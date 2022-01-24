use std::{
	ffi,
	fmt::{self, Display},
	ops::Deref,
	str::FromStr,
	time,
};

use humantime::{format_rfc3339, parse_rfc3339_weak, TimestampError};
use speedy::{self, Readable, Reader, Writable, Writer};

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
#[repr(transparent)]
pub struct OsString(ffi::OsString);

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
#[repr(transparent)]
pub struct SystemTime(time::SystemTime);

impl AsRef<ffi::OsString> for OsString {
	fn as_ref(&self) -> &ffi::OsString {
		&self.0
	}
}

impl Deref for OsString {
	type Target = ffi::OsString;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl From<ffi::OsString> for OsString {
	fn from(s: ffi::OsString) -> Self {
		Self(s)
	}
}

impl From<OsString> for ffi::OsString {
	fn from(s: OsString) -> Self {
		s.0
	}
}

impl<'a, C: speedy::Context> Readable<'a, C> for OsString {
	#[inline]
	fn read_from<R: Reader<'a, C>>(reader: &mut R) -> Result<Self, C::Error> {
		String::read_from(reader).map(Into::into).map(Self)
	}

	#[inline]
	fn minimum_bytes_needed() -> usize {
		<time::SystemTime as Readable<'a, C>>::minimum_bytes_needed()
	}
}

impl<C: speedy::Context> Writable<C> for OsString
where
	str: Writable<C>,
{
	#[inline]
	fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
		self.0.to_str().ok_or(speedy::Error::custom("OsString is not encodable as String"))?.write_to(writer)
	}

	#[inline]
	fn bytes_needed(&self) -> Result<usize, C::Error> {
		self.0.to_str().unwrap().bytes_needed()
	}
}

impl AsRef<time::SystemTime> for SystemTime {
	fn as_ref(&self) -> &time::SystemTime {
		&self.0
	}
}

impl Deref for SystemTime {
	type Target = time::SystemTime;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl From<time::SystemTime> for SystemTime {
	fn from(dur: time::SystemTime) -> Self {
		Self(dur)
	}
}

impl From<SystemTime> for time::SystemTime {
	fn from(dur: SystemTime) -> Self {
		dur.0
	}
}

impl FromStr for SystemTime {
	type Err = TimestampError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		parse_rfc3339_weak(s).map(SystemTime)
	}
}

impl Display for SystemTime {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		format_rfc3339(self.0).fmt(f)
	}
}

impl<'a, C: speedy::Context> Readable<'a, C> for SystemTime {
	#[inline]
	fn read_from<R: Reader<'a, C>>(reader: &mut R) -> Result<Self, C::Error> {
		time::SystemTime::read_from(reader).map(Self)
	}

	#[inline]
	fn minimum_bytes_needed() -> usize {
		<time::SystemTime as Readable<'a, C>>::minimum_bytes_needed()
	}
}

impl<C: speedy::Context> Writable<C> for SystemTime
where
	time::SystemTime: Writable<C>,
{
	#[inline]
	fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
		self.0.write_to(writer)
	}

	#[inline]
	fn bytes_needed(&self) -> Result<usize, C::Error> {
		self.0.bytes_needed()
	}
}
