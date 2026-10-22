use std::ffi::{c_char, c_void, CString};

#[link(name = "j", kind = "dylib")]
unsafe extern "C" {
    /// Initializes a new instance of the J language execution engine.
    /// Returns a raw pointer to the J session context.
    pub fn JInit() -> *mut c_void;

    /// Executes a text command string within the specified J session context.
    /// 0 = Success, other values denote specific J execution errors.
    pub fn JDo(handle: *mut c_void, command: *const c_char) -> i32;

    /// Shuts down the J instance and frees allocated session memory.
    pub fn JFree(handle: *mut c_void) -> i32;
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
            Self { handle }
        }
    }

    /// Evaluates a J expression string. 
    /// Returns Ok(i32) containing the J error/status code (0 = success).
    /// Returns Err(String) if the input contains an invalid interior null byte.
    pub fn eval(&self, command: &str) -> Result<i32, String> {
        // Convert the Rust string slice into a null-terminated C-string
        let c_command = CString::new(command)
            .map_err(|e| format!("Invalid string conversion: {}", e))?;
        
        unsafe {
            // Pass our raw session handle pointer and the C-string pointer to J
            let result_code = JDo(self.handle, c_command.as_ptr());
            Ok(result_code)
        }
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

        // 2. Execute a simple J assignment expression (assigning an array to variable 'a')
        // In J, 'a =: 1 2 3' defines a global array
        let result = engine.eval("a =: 1 2 3");
        
        // 3. Assert that string parsing completed and J returned a successful status code (0)
        assert!(result.is_ok(), "Rust failed to pass string to JDo");
        assert_eq!(result.unwrap(), 0, "J engine returned a non-zero error execution code");
    }
}

