/*
 * Tardygrada VM — Self-Healing
 * When things go wrong (hash mismatch, no consensus, corrupted data),
 * try to recover before returning an error.
 */

#include "heal.h"
#include "vm.h"
#include "memory.h"
#include "crypto.h"
#include <string.h>

int tardy_heal(void *vm_ptr, tardy_uuid_t agent_id, tardy_heal_action_t action)
{
    tardy_vm_t *vm = (tardy_vm_t *)vm_ptr;
    tardy_agent_t *agent = tardy_vm_find(vm, agent_id);
    if (!agent) return -1;

    switch (action) {
    case TARDY_HEAL_PROMOTE:
        if (agent->state == TARDY_STATE_STATIC)
            return tardy_vm_promote(vm, agent_id);
        return -1;
    case TARDY_HEAL_REVERIFY:
        /* Re-read and re-hash to check if value is actually fine */
        if (agent->state != TARDY_STATE_LIVE) return -1;
        if (!agent->memory.has_hash) return 0; /* no hash = nothing to verify */
        {
            char buf[4096];
            size_t sz = agent->data_size > sizeof(buf)
                        ? sizeof(buf) : agent->data_size;
            tardy_page_read(&agent->memory.primary, buf, sz);
            tardy_hash_t check;
            tardy_sha256(buf, sz, &check);
            if (tardy_hash_eq(&check, &agent->memory.birth_hash))
                return 0; /* value is fine */
        }
        /* Value corrupted — fall through to reconstruct */
        /* fall through */
    case TARDY_HEAL_RECONSTRUCT:
        if (agent->memory.replica_count <= 0) return -1;
        /* Read from replicas, majority vote, overwrite primary */
        {
            char buf[4096];
            size_t sz = agent->data_size > sizeof(buf)
                        ? sizeof(buf) : agent->data_size;
            tardy_read_status_t st = tardy_mem_read(&agent->memory, buf, sz);
            if (st == TARDY_READ_OK) {
                /* Replicas agreed — unlock primary, write correct value, relock */
                tardy_page_unlock(&agent->memory.primary);
                tardy_page_write(&agent->memory.primary, buf, sz);
                tardy_page_lock(&agent->memory.primary);
                return 0;
            }
        }
        return -1;
    }
    return -1;
}
