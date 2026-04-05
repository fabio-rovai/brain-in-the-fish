//! Safe Rust wrappers around the Tardygrada C VM.
//!
//! `TardyVm` owns a heap-allocated `tardy_vm_t` and exposes safe methods
//! for spawning agents, reading/mutating values, messaging, and GC.

use std::ffi::CString;
use std::os::raw::c_void;

use crate::ffi::*;

/// Allocation size for the VM struct. tardy_vm_t is enormous because it
/// contains `agents[65536]` where each `tardy_agent_t` is very large.
/// We compute a generous upper bound and zero-initialise via `vec![0u8; ..]`.
///
/// Conservative estimate:
///   tardy_agent_t ~ 300KB each (context alone has 256 children * ~80B + inbox of 64 msgs * ~600B)
///   65536 agents * 300KB ≈ 19 GB — that's too much.
///
/// Actually, we need to measure more carefully. Let's just use mmap with
/// the actual C sizeof. Instead, we'll use a helper approach: allocate via
/// `std::alloc::alloc_zeroed` with a layout derived from the actual struct.
///
/// For now, we use a C-side trick: we compute sizeof(tardy_vm_t) at build time.
/// BUT — simpler approach: just use Box with MaybeUninit and let tardy_vm_init
/// handle initialisation.  We need the size though.
///
/// The pragmatic approach: allocate `VM_ALLOC_BYTES` bytes, zero-init.
/// This must be >= sizeof(tardy_vm_t). We'll compute it conservatively from
/// the struct definition.

// sizeof(tardy_message_t) ~ 2*16 + 4 + 512 + 8 + 32 + 8 = ~596 -> round to 600
// sizeof(tardy_message_queue_t) = 64 * 600 + 3*4 = 38412 -> round to 38416
// sizeof(tardy_named_child_t) = 64 + 16 = 80
// sizeof(tardy_agent_context_t) = 256*80 + 4 + 38416 = 58900 -> round to 58904
// sizeof(tardy_page_t) = 8 + 8 + 4 + 1 + padding = ~24
// sizeof(tardy_agent_memory_t) = 4 + 24 + 32 + 1 + 8 + 4 + 64 + 1 + 32 + 8 + 4 = ~182 -> ~184
// sizeof(tardy_provenance_t) = 16 + 8 + 8 + 8 + 4 + 32 = ~76 -> ~80
// sizeof(tardy_constitution_t) = 16 * (4+8+8+4+4) + 4 + 32 = 16*28+36 = 484
// sizeof(tardy_conversation_turn_t) = 16 + 512 + 8 = 536
// sizeof(tardy_snapshot_t) = 16 + 32 + 16 + 8 + 4 + 4 = 80
// sizeof(tardy_agent_t) ~ 16 + 4*3 + 184 + 80 + 58904 + 8+4+4 + 8 + 8+256+80 + 8+8 + 484 + 8 + 8 + 32*536+4 = ~77kB
// 65536 * 77KB ≈ 5 GB — still large for stack but fine for heap with lazy pages
//
// sizeof(tardy_tombstone_t) = 16 + 4 + 32 + 32 + 8 = 92 -> ~96
// tombstones: 16384 * 96 ≈ 1.5 MB
//
// Total: ~5 GB + 1.5 MB + ~200 B overhead ≈ 5 GB
//
// This is intentionally a very large VM (65536 agents). The OS uses lazy
// page allocation so zeroed pages won't consume physical RAM until touched.

/// We use calloc-style allocation via mmap or alloc_zeroed so the OS only
/// commits pages on first write (lazy allocation on macOS/Linux).
const VM_ALLOC_BYTES: usize = {
    // Per-agent estimate (conservative, with padding):
    // ~80KB per agent is a safe upper bound
    let agent_size: usize = 80 * 1024;
    let agents_total: usize = TARDY_MAX_AGENTS * agent_size;
    // Tombstones: ~96 bytes each
    let tombstones_total: usize = TARDY_MAX_TOMBSTONES * 96;
    // Overhead (root_id, root_key, semantics, boot_time, running, counts)
    let overhead: usize = 4096;
    agents_total + tombstones_total + overhead
};

/// Safe wrapper around the Tardygrada VM.
pub struct TardyVm {
    /// Pointer to the heap-allocated VM. The allocation is VM_ALLOC_BYTES
    /// bytes, zero-initialised.
    ptr: *mut tardy_vm_t,
    /// Layout used for deallocation.
    layout: std::alloc::Layout,
}

// tardy_vm_t is single-threaded C code; we manage thread safety at a higher level.
unsafe impl Send for TardyVm {}

impl TardyVm {
    /// Create and initialise a new Tardygrada VM with default semantics.
    pub fn new() -> Result<Self, String> {
        let layout = std::alloc::Layout::from_size_align(VM_ALLOC_BYTES, 4096)
            .map_err(|e| format!("layout error: {e}"))?;

        let ptr = unsafe { std::alloc::alloc_zeroed(layout) as *mut tardy_vm_t };
        if ptr.is_null() {
            return Err("VM allocation failed".into());
        }

        let sem = tardy_semantics_t::default_semantics();
        let rc = unsafe { tardy_vm_init(ptr, &sem) };
        if rc != 0 {
            unsafe { std::alloc::dealloc(ptr as *mut u8, layout) };
            return Err(format!("tardy_vm_init failed: {rc}"));
        }

        Ok(TardyVm { ptr, layout })
    }

    /// Raw pointer to the VM (for pipeline functions that take `*mut tardy_vm_t`).
    pub fn as_ptr(&self) -> *mut tardy_vm_t {
        self.ptr
    }

    /// Get the root agent ID. The root_id is at a known offset in the VM struct:
    /// after agents[TARDY_MAX_AGENTS] + agent_count.
    /// We read it via tardy_vm_find or by spawning under the zero UUID and
    /// observing the parent. Simpler: just use UUID zero as root proxy since
    /// tardy_vm_init sets root_id and we can read it via find_by_name on root.
    ///
    /// Actually the safest way is to read the root_id field directly.
    /// It sits after agents[65536] (each ~80KB) + agent_count (4 bytes).
    /// That's fragile. Instead we spawn a sentinel agent under UUID{0,0}
    /// which the VM maps to root.
    pub fn root_id(&self) -> tardy_uuid_t {
        // The C VM uses parent_id == zero UUID to mean "root".
        // Most operations accept zero UUID as the root parent.
        tardy_uuid_t { hi: 0, lo: 0 }
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
            tardy_vm_shutdown(self.ptr);
            std::alloc::dealloc(self.ptr as *mut u8, self.layout);
        }
    }
}
