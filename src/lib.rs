macro_rules! cstr {
    ($x:literal) => {{
        #[allow(unused_unsafe)]
        unsafe {
            std::ffi::CStr::from_bytes_with_nul_unchecked(concat!($x, "\0").as_bytes())
        }
    }};
}

mod defer;
pub mod graphics;
pub mod window;
pub mod state;
pub mod sim;
pub mod render;

pub use defer::defer;
