//! Camada 1: bindings raw das DLLs (equivalente aos `external ... delayed`
//! do Delphi). As DLLs são carregadas sob demanda via `libloading`, então o
//! exe sobe (e o `startup_check` consegue mostrar a mensagem de "falta
//! arquivo") mesmo sem elas ao lado.

pub mod ffmpeg;
pub mod obs;

/// Declara uma tabela de funções de uma DLL e o `load` que resolve todas.
/// Falta de QUALQUER símbolo faz o carregamento inteiro falhar — melhor
/// descobrir no arranque do que no meio de uma gravação.
#[macro_export]
macro_rules! dll_api {
    (
        $(#[$meta:meta])*
        pub struct $Api:ident;
        $( fn $name:ident($($arg:ident : $ty:ty),* $(,)?) $(-> $ret:ty)?; )*
    ) => {
        $(#[$meta])*
        pub struct $Api {
            _lib: ::libloading::Library,
            $( pub $name: unsafe extern "C" fn($($ty),*) $(-> $ret)?, )*
        }

        impl $Api {
            /// Resolve todos os símbolos de `lib`.
            ///
            /// # Safety
            /// As assinaturas declaradas têm que casar com a DLL.
            pub unsafe fn load(lib: ::libloading::Library) -> Result<Self, ::libloading::Error> {
                Ok(Self {
                    $(
                        $name: *lib.get::<unsafe extern "C" fn($($ty),*) $(-> $ret)?>(
                            concat!(stringify!($name), "\0").as_bytes(),
                        )?,
                    )*
                    _lib: lib,
                })
            }
        }
    };
}

/// `*const c_char` → `String` (UTF-8, como o libobs e o FFmpeg; pegadinha #29).
///
/// # Safety
/// `p` nulo ou apontando para string C terminada em zero.
pub unsafe fn cstr_to_string(p: *const std::os::raw::c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}
