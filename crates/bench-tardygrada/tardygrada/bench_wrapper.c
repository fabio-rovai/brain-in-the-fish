/*
 * Bench Wrapper — thin C layer so Rust never needs to know sizeof(tardy_vm_t).
 *
 * The VM is allocated/freed on the C side with calloc/free.
 * Rust only holds an opaque pointer.
 */

#include "vm/vm.h"
#include <stdlib.h>
#include <string.h>

/* ── VM lifecycle ─────────────────────────────────────────────────── */

tardy_vm_t *tardy_bench_vm_create(void) {
    tardy_vm_t *vm = (tardy_vm_t *)calloc(1, sizeof(tardy_vm_t));
    if (!vm) return NULL;
    tardy_semantics_t sem = TARDY_DEFAULT_SEMANTICS;
    int rc = tardy_vm_init(vm, &sem);
    if (rc != 0) {
        free(vm);
        return NULL;
    }
    return vm;
}

void tardy_bench_vm_destroy(tardy_vm_t *vm) {
    if (vm) {
        tardy_vm_shutdown(vm);
        free(vm);
    }
}

/* ── Root ID accessor ─────────────────────────────────────────────── */

tardy_uuid_t tardy_bench_vm_root_id(const tardy_vm_t *vm) {
    if (!vm) {
        tardy_uuid_t z = {0, 0};
        return z;
    }
    return vm->root_id;
}
