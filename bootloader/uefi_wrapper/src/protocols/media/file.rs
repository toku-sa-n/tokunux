use super::simple_file_system::SimpleFileSystem;
use crate::result;
use core::convert::TryInto;
use core::mem;
use r_efi::efi::protocols::file;
use r_efi::efi::Status;

const FAT_MAX_NAME: usize = 255;

pub struct File<'a> {
    protocol: &'a mut file::Protocol,
    fs: &'a mut SimpleFileSystem,
}
impl<'a> File<'a> {
    pub fn open_read_only(&'a mut self, name: &'static str) -> crate::Result<File<'a>> {
        if name_too_long(name) {
            Err(Status::INVALID_PARAMETER.try_into())
        } else {
            self.open_read_only_unchecked(name)
        }
    }

    pub(crate) fn new(protocol: &'a mut file::Protocol, fs: &'a mut SimpleFileSystem) -> Self {
        Self { protocol, fs }
    }

    fn open_read_only_unchecked(&'a mut self, name: &'static str) -> crate::Result<File<'a>> {
        let mut name = name_to_u16_array(name);
        let mut new_handler = mem::MaybeUninit::uninit();

        let r = (self.protocol.open)(
            self.protocol,
            new_handler.as_mut_ptr(),
            name.as_mut_ptr(),
            file::READ_ONLY,
            0,
        );

        result::from_status_and_closure(r, move || {
            // SAFETY: `open` initializes `new_handler`.
            let new_handler = unsafe { new_handler.assume_init() };

            // SAFETY: Only one instance of `File` exists per `SimpleFileSystem`. Therefore there
            // is no mutable references which point to `*new_handler`.
            let new_handler = unsafe { &mut *new_handler };

            Self {
                protocol: new_handler,
                fs: self.fs,
            }
        })
    }
}

fn name_to_u16_array(name: &'static str) -> [u16; FAT_MAX_NAME + 1] {
    let mut buf = [0; FAT_MAX_NAME + 1];
    let r = ucs2::encode(name, &mut buf);

    r.expect("Failed to convert the file name to an u16 array.");

    buf
}

fn name_too_long(name: &str) -> bool {
    name.len() > FAT_MAX_NAME
}
