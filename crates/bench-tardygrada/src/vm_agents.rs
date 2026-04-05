//! Safe Rust wrappers around the Tardygrada C VM.
//!
//! `TardyVm` owns a C-allocated `tardy_vm_t` (via `tardy_bench_vm_create`)
//! and exposes safe methods for spawning agents, reading/mutating values,
//! messaging, and GC.
//!
//! The VM struct is enormous (~5 GB virtual) so it is allocated on the C side
//! with calloc. The OS uses lazy page allocation, so only touched pages consume
//! physical RAM.

use std::ffi::CString;
use std::os::raw::c_void;

use crate::ffi::*;

/// Safe wrapper around the Tardygrada VM.
///
/// The VM is allocated and freed entirely on the C side via
/// `tardy_bench_vm_create` / `tardy_bench_vm_destroy`.
pub struct TardyVm {
    /// Opaque pointer to C-allocated tardy_vm_t.
    ptr: *mut tardy_vm_t,
}

// tardy_vm_t is single-threaded C code; we manage thread safety at a higher level.
unsafe impl Send for TardyVm {}

impl TardyVm {
    /// Create and initialise a new Tardygrada VM with default semantics.
    ///
    /// The VM is allocated via calloc on the C side, so the OS lazily
    /// commits pages only when agents are actually spawned.
    pub fn new() -> Result<Self, String> {
        let ptr = unsafe { tardy_bench_vm_create() };
        if ptr.is_null() {
            return Err("tardy_bench_vm_create returned NULL".into());
        }
        Ok(TardyVm { ptr })
    }

    /// Raw pointer to the VM (for pipeline functions that take `*mut tardy_vm_t`).
    pub fn as_ptr(&self) -> *mut tardy_vm_t {
        self.ptr
    }

    /// Get the root agent ID by reading it from the C struct directly
    /// (via the bench wrapper, avoiding any offset assumptions).
    pub fn root_id(&self) -> tardy_uuid_t {
        unsafe { tardy_bench_vm_root_id(self.ptr) }
    }

    // ── Spawn helpers ─────────────────────────────────────────────

    /// Spawn a string agent.
    pub fn spawn_str(
        &self,
        parent: tardy_uuid_t,
        name: &str,
        value: &str,
        trust: tardy_trust_t,
    ) -> Result<tardy_uuid_t, String> {
        let c_name = CString::new(name).map_err(|e| e.to_string())?;
        let c_val = CString::new(value).map_err(|e| e.to_string())?;
        let id = unsafe {
            tardy_vm_spawn(
                self.ptr,
                parent,
                c_name.as_ptr(),
                tardy_type_t::TARDY_TYPE_STR,
                trust,
                c_val.as_ptr() as *const c_void,
                value.len() + 1, // include null terminator
            )
        };
        if id.is_zero() {
            Err(format!("spawn_str failed for '{name}'"))
        } else {
            Ok(id)
        }
    }

    /// Spawn an integer agent.
    pub fn spawn_int(
        &self,
        parent: tardy_uuid_t,
        name: &str,
        value: i64,
        trust: tardy_trust_t,
    ) -> Result<tardy_uuid_t, String> {
        let c_name = CString::new(name).map_err(|e| e.to_string())?;
        let id = unsafe {
            tardy_vm_spawn(
                self.ptr,
                parent,
                c_name.as_ptr(),
                tardy_type_t::TARDY_TYPE_INT,
                trust,
                &value as *const i64 as *const c_void,
                std::mem::size_of::<i64>(),
            )
        };
        if id.is_zero() {
            Err(format!("spawn_int failed for '{name}'"))
        } else {
            Ok(id)
        }
    }

    /// Spawn a float agent.
    pub fn spawn_float(
        &self,
        parent: tardy_uuid_t,
        name: &str,
        value: f64,
        trust: tardy_trust_t,
    ) -> Result<tardy_uuid_t, String> {
        let c_name = CString::new(name).map_err(|e| e.to_string())?;
        let id = unsafe {
            tardy_vm_spawn(
                self.ptr,
                parent,
                c_name.as_ptr(),
                tardy_type_t::TARDY_TYPE_FLOAT,
                trust,
                &value as *const f64 as *const c_void,
                std::mem::size_of::<f64>(),
            )
        };
        if id.is_zero() {
            Err(format!("spawn_float failed for '{name}'"))
        } else {
            Ok(id)
        }
    }

    /// Spawn a fact agent (grounded claim with evidence text).
    pub fn spawn_fact(
        &self,
        parent: tardy_uuid_t,
        name: &str,
        evidence: &str,
        trust: tardy_trust_t,
    ) -> Result<tardy_uuid_t, String> {
        let c_name = CString::new(name).map_err(|e| e.to_string())?;
        let c_ev = CString::new(evidence).map_err(|e| e.to_string())?;
        let id = unsafe {
            tardy_vm_spawn(
                self.ptr,
                parent,
                c_name.as_ptr(),
                tardy_type_t::TARDY_TYPE_FACT,
                trust,
                c_ev.as_ptr() as *const c_void,
                evidence.len() + 1,
            )
        };
        if id.is_zero() {
            Err(format!("spawn_fact failed for '{name}'"))
        } else {
            Ok(id)
        }
    }

    // ── Read helpers ──────────────────────────────────────────────

    /// Read a string agent's value.
    pub fn read_str(
        &self,
        parent: tardy_uuid_t,
        name: &str,
    ) -> Result<String, String> {
        let c_name = CString::new(name).map_err(|e| e.to_string())?;
        let mut buf = [0u8; 4096];
        let status = unsafe {
            tardy_vm_read(
                self.ptr,
                parent,
                c_name.as_ptr(),
                buf.as_mut_ptr() as *mut c_void,
                buf.len(),
            )
        };
        match status {
            tardy_read_status_t::TARDY_READ_OK => {
                // Find null terminator
                let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
                String::from_utf8(buf[..end].to_vec())
                    .map_err(|e| format!("utf8 error: {e}"))
            }
            other => Err(format!("read_str '{name}' failed: {other:?}")),
        }
    }

    /// Read a float agent's value.
    pub fn read_float(
        &self,
        parent: tardy_uuid_t,
        name: &str,
    ) -> Result<f64, String> {
        let c_name = CString::new(name).map_err(|e| e.to_string())?;
        let mut val: f64 = 0.0;
        let status = unsafe {
            tardy_vm_read(
                self.ptr,
                parent,
                c_name.as_ptr(),
                &mut val as *mut f64 as *mut c_void,
                std::mem::size_of::<f64>(),
            )
        };
        match status {
            tardy_read_status_t::TARDY_READ_OK => Ok(val),
            other => Err(format!("read_float '{name}' failed: {other:?}")),
        }
    }

    // ── Mutate ────────────────────────────────────────────────────

    /// Mutate a mutable float agent's value.
    pub fn mutate_float(
        &self,
        parent: tardy_uuid_t,
        name: &str,
        value: f64,
    ) -> Result<(), String> {
        let c_name = CString::new(name).map_err(|e| e.to_string())?;
        let rc = unsafe {
            tardy_vm_mutate(
                self.ptr,
                parent,
                c_name.as_ptr(),
                &value as *const f64 as *const c_void,
                std::mem::size_of::<f64>(),
            )
        };
        if rc == 0 {
            Ok(())
        } else {
            Err(format!("mutate_float '{name}' failed: {rc}"))
        }
    }

    // ── Messaging ─────────────────────────────────────────────────

    /// Send a string message from one agent to another.
    pub fn send(&self, from: tardy_uuid_t, to: tardy_uuid_t, payload: &str) -> Result<(), String> {
        let c_payload = CString::new(payload).map_err(|e| e.to_string())?;
        let rc = unsafe {
            tardy_vm_send(
                self.ptr,
                from,
                to,
                c_payload.as_ptr() as *const c_void,
                payload.len() + 1,
                tardy_type_t::TARDY_TYPE_STR,
            )
        };
        if rc == 0 {
            Ok(())
        } else {
            Err(format!("send failed: {rc}"))
        }
    }

    /// Receive the next message from an agent's inbox.
    /// Returns None if inbox is empty.
    pub fn recv(&self, agent_id: tardy_uuid_t) -> Option<String> {
        let mut msg = std::mem::MaybeUninit::<tardy_message_t>::zeroed();
        let rc = unsafe { tardy_vm_recv(self.ptr, agent_id, msg.as_mut_ptr()) };
        if rc != 0 {
            return None;
        }
        let msg = unsafe { msg.assume_init() };
        // Extract payload as string
        let payload_bytes: Vec<u8> = msg.payload[..msg.payload_len]
            .iter()
            .map(|&c| c as u8)
            .collect();
        let end = payload_bytes
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(payload_bytes.len());
        String::from_utf8(payload_bytes[..end].to_vec()).ok()
    }

    // ── Freeze ────────────────────────────────────────────────────

    /// Promote an agent's trust level (freeze).
    pub fn freeze(
        &self,
        agent_id: tardy_uuid_t,
        new_trust: tardy_trust_t,
    ) -> Result<tardy_uuid_t, String> {
        let id = unsafe { tardy_vm_freeze(self.ptr, agent_id, new_trust) };
        if id.is_zero() {
            Err("freeze failed".into())
        } else {
            Ok(id)
        }
    }

    // ── GC ────────────────────────────────────────────────────────

    /// Run one garbage collection cycle.
    pub fn gc(&self) -> i32 {
        unsafe { tardy_vm_gc(self.ptr) }
    }
}

impl Drop for TardyVm {
    fn drop(&mut self) {
        unsafe {
            tardy_bench_vm_destroy(self.ptr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_create_and_spawn() {
        let vm = TardyVm::new().expect("VM creation should succeed");

        // Root ID should be non-zero (vm_init generates a random UUID for root)
        let root = vm.root_id();
        assert!(!root.is_zero(), "root_id should be non-zero");

        // Spawn a string agent under root
        let agent_id = vm
            .spawn_str(root, "hello", "world", tardy_trust_t::TARDY_TRUST_DEFAULT)
            .expect("spawn_str should succeed");
        assert!(!agent_id.is_zero(), "spawned agent UUID should be non-zero");

        // Read it back
        let val = vm.read_str(root, "hello").expect("read_str should succeed");
        assert_eq!(val, "world");
    }

    #[test]
    fn test_spawn_int_and_float() {
        let vm = TardyVm::new().expect("VM creation should succeed");
        let root = vm.root_id();

        let int_id = vm
            .spawn_int(root, "answer", 42, tardy_trust_t::TARDY_TRUST_DEFAULT)
            .expect("spawn_int should succeed");
        assert!(!int_id.is_zero());

        let float_id = vm
            .spawn_float(root, "pi", 3.14159, tardy_trust_t::TARDY_TRUST_MUTABLE)
            .expect("spawn_float should succeed");
        assert!(!float_id.is_zero());

        let pi = vm.read_float(root, "pi").expect("read_float should succeed");
        assert!((pi - 3.14159).abs() < 1e-10);
    }

    #[test]
    fn test_mutate_float() {
        let vm = TardyVm::new().expect("VM creation should succeed");
        let root = vm.root_id();

        vm.spawn_float(root, "score", 0.5, tardy_trust_t::TARDY_TRUST_MUTABLE)
            .expect("spawn_float should succeed");

        vm.mutate_float(root, "score", 0.9)
            .expect("mutate_float should succeed");

        let val = vm.read_float(root, "score").expect("read after mutate");
        assert!((val - 0.9).abs() < 1e-10);
    }
}
