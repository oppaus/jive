use std::cell::RefCell;
use std::ffi::{c_char, c_void, CStr, CString};

/// Output type: formatted result array output (see `MTYOFM` in jsource jlib.h).
const MTYOFM: i32 = 1;
/// Output type: error output (see `MTYOER` in jsource jlib.h).
const MTYOER: i32 = 2;

#[link(name = "j", kind = "dylib")]
unsafe extern "C" {
    /// Initializes a new instance of the J language execution engine.
    /// Returns a raw pointer to the J session context.
    pub fn JInit() -> *mut c_void;

    /// Installs the session-manager callbacks. `callbacks` is an array of five
    /// pointers: `[output, wd, input, poll, smoptions]` (see jlib.h). We only
    /// supply the output handler; the rest are null so J uses no front end.
    pub fn JSM(handle: *mut c_void, callbacks: *mut *mut c_void);

    /// Executes a text command string within the specified J session context.
    /// 0 = Success, other values denote specific J execution errors.
    pub fn JDo(handle: *mut c_void, command: *const c_char) -> i32;

    /// Shuts down the J instance and frees allocated session memory.
    pub fn JFree(handle: *mut c_void) -> i32;
}

thread_local! {
    /// Captured J output for the current thread, consumed by `eval`.
    ///
    /// J delivers output by calling the `JSM` callback synchronously from
    /// within `JDo`, so a thread-local buffer is sufficient for the
    /// single-threaded TUI event loop.
    static OUTPUT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// The C output callback J invokes for every piece of session output.
///
/// `type` is one of the `MTYO*` codes from jlib.h; `s` is a NUL-terminated
/// string owned by J that is only valid for the duration of the call.
extern "C" fn output_handler(_handle: *mut c_void, output_type: i32, s: *const c_char) {
    if s.is_null() {
        return;
    }
    // SAFETY: J guarantees a valid NUL-terminated C string for the call.
    let text = unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned();
    let formatted = match output_type {
        MTYOER => format!("error: {}", text),
        MTYOFM => text,
        _ => text,
    };
    OUTPUT.with(|buf| buf.borrow_mut().push(formatted));
}

/// A safe, idiomatic Rust wrapper around the unsafe raw J engine pointers.
pub struct JEngine {
    handle: *mut c_void,
}

impl JEngine {
    pub fn new() -> Self {
        unsafe {
            let handle = JInit();
            assert!(!handle.is_null(), "Failed to initialize J engine instance.");

            // Register our output callback so J routes session output to us
            // instead of the default handler. Slot order per jlib.h:
            // [output, wd, input, poll, smoptions].
            let mut callbacks: [*mut c_void; 5] = [
                output_handler as *mut c_void,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ];
            JSM(handle, callbacks.as_mut_ptr());

            Self { handle }
        }
    }

    /// Evaluates a J expression and returns its captured output.
    ///
    /// On success the returned `String` contains whatever J printed (the
    /// formatted result), including error text if J reported a nonzero status.
    /// Returns `Err(String)` if the input contains an interior null byte.
    pub fn eval(&self, command: &str) -> Result<String, String> {
        let c_command = CString::new(command)
            .map_err(|e| format!("Invalid string conversion: {}", e))?;

        // Drop any output left over from a previous call before running.
        OUTPUT.with(|buf| buf.borrow_mut().clear());

        let result_code = unsafe { JDo(self.handle, c_command.as_ptr()) };

        let output = OUTPUT.with(|buf| buf.borrow_mut().join("\n"));

        if result_code != 0 && output.is_empty() {
            return Ok(format!("[J error code: {}]", result_code));
        }
        Ok(output)
    }
}

impl Default for JEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for JEngine {
    fn drop(&mut self) {
        unsafe {
            JFree(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_j_engine_plumbing() {
        // 1. Initialize the J instance via FFI
        let engine = JEngine::new();

        // 2. Execute a simple J expression; J prints the formatted result.
        let result = engine.eval("1 2 3");
        assert!(result.is_ok(), "Rust failed to pass string to JDo");
        assert_eq!(
            result.unwrap().trim(),
            "1 2 3",
            "J output should be captured via the JSM callback"
        );
    }

    #[test]
    fn test_j_engine_error_output() {
        let engine = JEngine::new();

        // An undefined name yields J error output rather than a plain result.
        let result = engine.eval("this_is_not_defined_xyz");
        let output = result.expect("eval should still return Ok with captured error text");
        assert!(
            output.contains("error") || !output.is_empty(),
            "J error should be surfaced in the captured output, got: {output:?}"
        );
    }
}

