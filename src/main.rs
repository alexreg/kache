#![feature(exit_status_error)]
#![feature(let_chains)]
#![feature(try_blocks)]
#![feature(type_alias_impl_trait)]

mod archivable;

use std::{
	collections::hash_map::DefaultHasher,
	env::temp_dir,
	ffi::OsString,
	fs::{self, File},
	hash::{Hash, Hasher},
	io::{self, Read, Write},
	mem,
	path::{Path, PathBuf},
	process::{self, Command, Stdio},
	time,
};

use anyhow::{anyhow, bail, Result};
use clap::Parser;
use humantime::{Duration, Timestamp as SystemTime};
use speedy::{Readable, Writable};

const CACHE_DIR: &'static str = "kache";

#[derive(Debug, Hash, PartialEq, Eq)]
struct CacheEntry {
	id: String,
	info_path: PathBuf,
	stdout_path: PathBuf,
	stderr_path: PathBuf,
}

#[derive(Debug, PartialEq, Readable, Writable)]
struct CacheEntryInfo {
	command: Vec<archivable::OsString>,
	expiry: Option<archivable::SystemTime>,
	exit_code: i32,
}

// TODO: Use argument groups.
// TODO: Add verbose flag and logging (see Rust CLI Book).
/// Cache the output of a program.
#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
	/// Check for cache entry
	#[arg(short, long, conflicts_with = "remove", conflicts_with = "clear")]
	check: bool,

	/// Remove cache entry
	#[arg(short, long, conflicts_with = "check", conflicts_with = "clear")]
	remove: bool,

	/// Remove entire cache
	#[arg(long, conflicts_with = "remove", conflicts_with = "check")]
	clear: bool,

	/// Purge cache of expired entries
	#[arg(long, conflicts_with = "remove", conflicts_with = "check")]
	purge: bool,

	/// Ignore pre-existing cache
	#[arg(short, long, conflicts_with = "force")]
	ignore: bool,

	/// Force use of pre-existing cache
	#[arg(short, long, conflicts_with = "ignore")]
	force: bool,

	/// Date-time of expiry
	#[arg(short, long, conflicts_with = "duration")]
	expiry: Option<SystemTime>,

	/// Duration before expiry
	#[arg(short, long, conflicts_with = "expiry")]
	duration: Option<Duration>,

	/// Program and arguments to run
	#[arg(trailing_var_arg = true)]
	command: Vec<OsString>,
}

impl CacheEntry {
	fn cache_dir() -> PathBuf {
		temp_dir().join(CACHE_DIR)
	}

	fn new<C: AsRef<[OsString]> + Hash>(command: C) -> Result<Self> {
		let mut hasher = DefaultHasher::new();
		command.hash(&mut hasher);
		let id = format!("{:016x}", hasher.finish());

		let dir_path = Self::cache_dir();
		fs::create_dir_all(&dir_path)?;
		let info_path = dir_path.join(&id);
		let stdout_path = info_path.with_extension("stdout");
		let stderr_path = info_path.with_extension("stderr");

		Ok(Self {
			id,
			info_path,
			stdout_path,
			stderr_path,
		})
	}

	fn load<P: AsRef<Path>>(info_path: P) -> Result<Self> {
		let id = info_path.as_ref().file_name().ok_or_else(|| anyhow!("`info_path` is not file"))?.to_string_lossy().into_owned();
		let info_path = info_path.as_ref().to_owned();

		let stdout_path = info_path.with_extension("stdout");
		let stderr_path = info_path.with_extension("stderr");

		Ok(Self {
			id,
			info_path,
			stdout_path,
			stderr_path,
		})
	}

	fn exists(&self) -> bool {
		self.info_path.exists()
	}

	fn remove(&self) -> io::Result<()> {
		fs::remove_file(&self.info_path)?;
		fs::remove_file(&self.stdout_path)?;
		fs::remove_file(&self.stderr_path)?;
		Ok(())
	}

	fn read_info(&self) -> io::Result<CacheEntryInfo> {
		let mut info_file = File::open(&self.info_path)?;
		Ok(CacheEntryInfo::read_from_stream_unbuffered(&mut info_file)?)
	}

	fn read_stdout(&self) -> io::Result<File> {
		Ok(File::open(&self.stdout_path)?)
	}

	fn read_stderr(&self) -> io::Result<File> {
		Ok(File::open(&self.stderr_path)?)
	}

	fn write_info(&self, info: &'_ CacheEntryInfo) -> io::Result<()> {
		let mut info_file = File::create(&self.info_path)?;
		info.write_to_stream(&mut info_file)?;
		Ok(())
	}

	fn write_stdout(&self) -> io::Result<File> {
		Ok(File::create(&self.stdout_path)?)
	}

	fn write_stderr(&self) -> io::Result<File> {
		Ok(File::create(&self.stderr_path)?)
	}
}

impl CacheEntryInfo {
	fn new(command: Vec<OsString>, expiry: Option<SystemTime>, exit_code: i32) -> Self {
		Self {
			command: unsafe { mem::transmute(command) },
			expiry: unsafe { mem::transmute(expiry) },
			exit_code,
		}
	}

	fn valid(&self) -> bool {
		self.expiry.map_or(true, |expiry| time::SystemTime::now() < *expiry)
	}
}

fn clear() -> Result<()> {
	let cache_dir = CacheEntry::cache_dir();
	match fs::remove_dir_all(&cache_dir) {
		Ok(()) => {
			println!("Cache directory removed: {}", cache_dir.display());
			Ok(())
		},
		Err(err) if err.kind() == io::ErrorKind::NotFound => Err(anyhow!("Cache directory not found")),
		Err(err) => Err(err.into()),
	}
}

fn purge() -> Result<()> {
	let cache_dir = CacheEntry::cache_dir();
	match fs::read_dir(&cache_dir) {
		Ok(dir) => {
			for entry in dir {
				let entry = entry?;
				let path = entry.path();
				if entry.file_type()?.is_file() && path.extension().is_none() {
					let cache_entry = CacheEntry::load(&path)?;
					let info = cache_entry.read_info()?;

					if !info.valid() {
						println!("Purging cache entry {}...", cache_entry.id);
						cache_entry.remove()?;
					}
				}
			}
			println!("Cache directory purged of expired entries: {}", cache_dir.display());
			Ok(())
		},
		Err(err) if err.kind() == io::ErrorKind::NotFound => Err(anyhow!("Cache directory not found")),
		Err(err) => Err(err.into()),
	}
}

fn main() -> Result<()> {
	let cli = Cli::parse();

	if cli.clear {
		return clear();
	}

	if cli.purge {
		return purge();
	}

	let cache_entry = CacheEntry::new(&cli.command)?;

	if cli.check {
		if cache_entry.exists() {
			let info = cache_entry.read_info()?;
			let valid_str = if info.valid() {
				"valid"
			} else {
				"expired"
			};
			println!("Cache entry found: {} ({})", cache_entry.info_path.display(), valid_str);
			return Ok(());
		} else {
			bail!("Cache entry not found");
		}
	}

	if cli.remove {
		match cache_entry.remove() {
			Ok(()) => {
				println!("Cache entry removed: {}", cache_entry.info_path.display());
				return Ok(());
			},
			Err(err) if err.kind() == io::ErrorKind::NotFound => {
				bail!("Cache entry not found");
			},
			Err(err) => Err(err)?,
		}
	}

	let expiry = cli.expiry.or_else(|| cli.duration.map(|duration| SystemTime::from(time::SystemTime::now() + *duration)));

	let (program, args) = cli.command.split_first().ok_or(anyhow!("No command specified"))?;
	let program: &OsString = unsafe { mem::transmute(program) };
	let args: &[OsString] = unsafe { mem::transmute(args) };

	let mut stdout = io::stdout();
	let mut stderr = io::stderr();

	let mut buf = [0; 0xFFFF];

	let info = match cache_entry.read_info() {
		Ok(info) => {
			if info.valid() {
				Some(info)
			} else {
				cache_entry.remove()?;
				None
			}
		},
		Err(err) if err.kind() == io::ErrorKind::NotFound => None,
		Err(err) => Err(err)?,
	};

	// Check if cache exists and has not yet expired.
	if let Some(info) = info && info.valid() {
		let mut stdout_cache = cache_entry.read_stdout()?;
		let mut stderr_cache = cache_entry.read_stderr()?;

		io::copy(&mut stdout_cache, &mut stdout)?;
		io::copy(&mut stderr_cache, &mut stderr)?;

		process::exit(info.exit_code);
	} else {
		let mut stdout_cache = cache_entry.write_stdout()?;
		let mut stderr_cache = cache_entry.write_stderr()?;

		let mut child = Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;

		let mut child_stdout = child.stdout.take().unwrap();
		let mut child_stderr = child.stderr.take().unwrap();

		let child_status = loop {
			let stdout_count = child_stdout.read(&mut buf)?;
			stdout.write_all(&buf[..stdout_count])?;
			stdout_cache.write_all(&buf[..stdout_count])?;

			let stderr_count = child_stderr.read(&mut buf)?;
			stderr.write_all(&buf[..stderr_count])?;
			stderr_cache.write_all(&buf[..stderr_count])?;

			if stdout_count == 0 && stderr_count == 0 && let Some(status) = child.try_wait()? {
				break status;
			}
		};

		if let Some(exit_code) = child_status.code() {
			let info = CacheEntryInfo::new(cli.command, expiry, exit_code);
			cache_entry.write_info(&info)?;

			process::exit(exit_code);
		} else {
			cache_entry.remove()?;
		}
	}

	Ok(())
}
