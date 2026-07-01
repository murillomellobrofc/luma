//! Autenticação via PAM (Pluggable Authentication Modules) carregada
//! em runtime via libloading. Mesmo padrão da libcanberra (H54.P6):
//! sem dev headers no build, falha em silêncio se a `.so` não estiver
//! presente.
//!
//! H58 fatia 5. Roda numa thread separada — main loop nunca bloqueia
//! aguardando PAM. Outcome chega num Arc<Mutex<Option<AuthResult>>>.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::sync::{Arc, Mutex};

use libloading::{Library, Symbol};

// ─── PAM types (opaque) ───────────────────────────────────────────────
#[repr(C)]
struct PamHandle {
    _private: [u8; 0],
}

// pam_message style constants
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_ERROR_MSG: c_int = 3;
const PAM_TEXT_INFO: c_int = 4;

// pam_authenticate return codes
const PAM_SUCCESS: c_int = 0;

#[repr(C)]
struct PamMessage {
    msg_style: c_int,
    msg: *const c_char,
}

#[repr(C)]
struct PamResponse {
    resp: *mut c_char,
    resp_retcode: c_int,
}

#[repr(C)]
struct PamConv {
    conv: unsafe extern "C" fn(
        num_msg: c_int,
        msg: *mut *const PamMessage,
        resp: *mut *mut PamResponse,
        appdata_ptr: *mut c_void,
    ) -> c_int,
    appdata_ptr: *mut c_void,
}

// FFI signatures
type PamStart = unsafe extern "C" fn(
    service: *const c_char,
    user: *const c_char,
    conv: *const PamConv,
    pamh: *mut *mut PamHandle,
) -> c_int;
type PamAuthenticate = unsafe extern "C" fn(pamh: *mut PamHandle, flags: c_int) -> c_int;
type PamAcctMgmt = unsafe extern "C" fn(pamh: *mut PamHandle, flags: c_int) -> c_int;
type PamEnd = unsafe extern "C" fn(pamh: *mut PamHandle, status: c_int) -> c_int;

/// Resultado da auth (visto pelo main loop via outbox).
#[derive(Debug, Clone)]
pub enum AuthResult {
    Success,
    Failure { reason: String },
}

/// Sobrescreve os bytes com zero via escrita volátil (o compilador não
/// pode otimizar embora) e esvazia. Best-effort — não alcança cópias
/// que o PAM faz internamente, mas apaga os buffers que controlamos.
pub fn wipe_string(s: &mut String) {
    // SAFETY: escrevemos len bytes válidos dentro da capacidade do
    // buffer; depois zeramos o len via clear().
    unsafe {
        let bytes = s.as_bytes_mut();
        for b in bytes.iter_mut() {
            std::ptr::write_volatile(b, 0);
        }
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    s.clear();
}

fn wipe_bytes(bytes: &mut [u8]) {
    for b in bytes.iter_mut() {
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

/// Conversation callback fixo. PAM chama esse pra cada prompt; pra
/// PAM_PROMPT_ECHO_OFF (senha), devolve a senha; pros outros, vazio.
///
/// appdata_ptr é o ponteiro pro `CString` da senha.
unsafe extern "C" fn pam_conv_callback(
    num_msg: c_int,
    msg: *mut *const PamMessage,
    resp_out: *mut *mut PamResponse,
    appdata: *mut c_void,
) -> c_int {
    if num_msg <= 0 || msg.is_null() || resp_out.is_null() {
        return -1;
    }
    let password = &*(appdata as *const CString);
    // PAM espera que a gente aloque via malloc, ele libera com free.
    let resp_array = libc::calloc(num_msg as usize, std::mem::size_of::<PamResponse>())
        as *mut PamResponse;
    if resp_array.is_null() {
        return -1;
    }
    for i in 0..num_msg as isize {
        let m_ptr = *msg.offset(i);
        if m_ptr.is_null() {
            continue;
        }
        let m = &*m_ptr;
        let r = resp_array.offset(i);
        (*r).resp_retcode = 0;
        match m.msg_style {
            PAM_PROMPT_ECHO_OFF | PAM_PROMPT_ECHO_ON => {
                // Devolve a senha como string nova (PAM dá free()).
                (*r).resp = libc::strdup(password.as_ptr());
            }
            PAM_ERROR_MSG | PAM_TEXT_INFO => {
                (*r).resp = std::ptr::null_mut();
            }
            _ => {
                (*r).resp = std::ptr::null_mut();
            }
        }
    }
    *resp_out = resp_array;
    PAM_SUCCESS
}

/// Carrega libpam + entradas. None se libpam não disponível.
struct PamLib {
    _lib: Library,
    pam_start: PamStart,
    pam_authenticate: PamAuthenticate,
    pam_acct_mgmt: PamAcctMgmt,
    pam_end: PamEnd,
}

impl PamLib {
    fn try_load() -> Option<Self> {
        let lib = unsafe {
            Library::new("libpam.so.0")
                .or_else(|_| Library::new("libpam.so"))
                .ok()?
        };
        let pam_start: Symbol<PamStart> =
            unsafe { lib.get(b"pam_start").ok()? };
        let pam_authenticate: Symbol<PamAuthenticate> =
            unsafe { lib.get(b"pam_authenticate").ok()? };
        let pam_acct_mgmt: Symbol<PamAcctMgmt> =
            unsafe { lib.get(b"pam_acct_mgmt").ok()? };
        let pam_end: Symbol<PamEnd> = unsafe { lib.get(b"pam_end").ok()? };
        let start = *pam_start;
        let auth = *pam_authenticate;
        let acct = *pam_acct_mgmt;
        let end = *pam_end;
        drop(pam_start);
        drop(pam_authenticate);
        drop(pam_acct_mgmt);
        drop(pam_end);
        Some(Self {
            _lib: lib,
            pam_start: start,
            pam_authenticate: auth,
            pam_acct_mgmt: acct,
            pam_end: end,
        })
    }
}

/// Auth síncrona (usar SEMPRE numa thread). Retorna AuthResult.
fn authenticate_sync(user: &str, password: &str, service: &str) -> AuthResult {
    let lib = match PamLib::try_load() {
        Some(l) => l,
        None => {
            return AuthResult::Failure {
                reason: "libpam não encontrada".into(),
            };
        }
    };
    let service_c = match CString::new(service) {
        Ok(c) => c,
        Err(_) => return AuthResult::Failure { reason: "service inválido".into() },
    };
    let user_c = match CString::new(user) {
        Ok(c) => c,
        Err(_) => return AuthResult::Failure { reason: "user inválido".into() },
    };
    let password_c = match CString::new(password) {
        Ok(c) => c,
        Err(_) => return AuthResult::Failure { reason: "senha inválida".into() },
    };
    let password_box = Box::new(password_c);
    let conv = PamConv {
        conv: pam_conv_callback,
        appdata_ptr: (&*password_box as *const CString) as *mut c_void,
    };
    let mut pamh: *mut PamHandle = std::ptr::null_mut();
    let start_rc = unsafe {
        (lib.pam_start)(
            service_c.as_ptr(),
            user_c.as_ptr(),
            &conv as *const PamConv,
            &mut pamh,
        )
    };
    if start_rc != PAM_SUCCESS || pamh.is_null() {
        let mut pw_bytes = (*password_box).into_bytes();
        wipe_bytes(&mut pw_bytes);
        return AuthResult::Failure {
            reason: format!("pam_start rc={start_rc}"),
        };
    }
    let auth_rc = unsafe { (lib.pam_authenticate)(pamh, 0) };
    // Só validamos a conta se a senha conferiu. pam_acct_mgmt barra
    // contas expiradas/bloqueadas/desabilitadas por política que ainda
    // tenham hash de senha válido — sem isso, elas destravariam.
    let acct_rc = if auth_rc == PAM_SUCCESS {
        unsafe { (lib.pam_acct_mgmt)(pamh, 0) }
    } else {
        auth_rc
    };
    unsafe { (lib.pam_end)(pamh, acct_rc) };
    // Apaga a cópia da senha que controlamos (o strdup interno do PAM
    // já foi liberado por pam_end; esse não conseguimos alcançar).
    let mut pw_bytes = (*password_box).into_bytes();
    wipe_bytes(&mut pw_bytes);
    if auth_rc == PAM_SUCCESS && acct_rc == PAM_SUCCESS {
        AuthResult::Success
    } else {
        // Reporta a primeira falha: senha errada tem prioridade sobre
        // problema de conta.
        let rc = if auth_rc != PAM_SUCCESS { auth_rc } else { acct_rc };
        AuthResult::Failure {
            reason: pam_error_label(rc).into(),
        }
    }
}

fn pam_error_label(rc: c_int) -> &'static str {
    match rc {
        7 => "senha incorreta",
        9 => "credenciais inválidas",
        13 => "usuário desconhecido",
        25 => "tempo esgotado",
        _ => "falha na autenticação",
    }
}

/// Nome do usuário dono da sessão, derivado do UID real via
/// getpwuid(getuid()). NÃO usa $USER, que é spoofável e pode estar
/// ausente. None se getpwuid falhar — o chamador deve tratar como
/// falha de auth, nunca cair num usuário padrão.
pub fn current_username() -> Option<String> {
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() {
            return None;
        }
        let name_ptr = (*pw).pw_name;
        if name_ptr.is_null() {
            return None;
        }
        // pw aponta pra storage estático da libc; copiamos na hora.
        CStr::from_ptr(name_ptr).to_str().ok().map(str::to_owned)
    }
}

/// Dispara auth em thread separada. Outcome no slot compartilhado.
/// H58 fatia 5.
pub fn authenticate_async(
    outbox: Arc<Mutex<Option<AuthResult>>>,
    user: String,
    password: String,
) {
    std::thread::Builder::new()
        .name("luma-pam-auth".into())
        .spawn(move || {
            let mut password = password;
            // Service "login" é universal nas distros. Considerar
            // criar /etc/pam.d/luma futuramente pra service próprio.
            let result = authenticate_sync(&user, &password, "login");
            wipe_string(&mut password);
            tracing::info!(
                "pam auth: user={user} result={:?}",
                std::mem::discriminant(&result),
            );
            if let Ok(mut g) = outbox.lock() {
                *g = Some(result);
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wipe_string_zeroes_and_empties() {
        let mut s = String::from("senha-secreta-123");
        let cap = s.capacity();
        wipe_string(&mut s);
        assert!(s.is_empty(), "buffer deve ficar vazio");
        // A capacidade permanece; os bytes subjacentes foram zerados
        // antes do clear. Reexpõe o buffer e confere que não sobrou
        // nada do plaintext.
        unsafe {
            let v = s.as_mut_vec();
            v.set_len(cap.min(17));
            assert!(v.iter().all(|&b| b == 0), "bytes residuais não zerados");
            v.set_len(0);
        }
    }

    #[test]
    fn wipe_bytes_zeroes_all() {
        let mut b = *b"topsecret";
        wipe_bytes(&mut b);
        assert!(b.iter().all(|&x| x == 0));
    }

    #[test]
    fn current_username_is_non_empty_when_present() {
        // Não deve entrar em pânico; se resolver, é não-vazio e nunca
        // deve cair silenciosamente em algo como "root" (o antigo bug
        // era fallback pra $USER/"root").
        if let Some(u) = current_username() {
            assert!(!u.is_empty());
        }
    }

    #[test]
    fn pam_error_label_maps_known_codes() {
        assert_eq!(pam_error_label(7), "senha incorreta");
        assert_eq!(pam_error_label(13), "usuário desconhecido");
        assert_eq!(pam_error_label(9999), "falha na autenticação");
    }
}
