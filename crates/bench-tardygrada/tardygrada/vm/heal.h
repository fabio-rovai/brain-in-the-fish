#ifndef TARDY_HEAL_H
#define TARDY_HEAL_H

#include "types.h"

typedef enum {
    TARDY_HEAL_PROMOTE,      /* promote static back to live */
    TARDY_HEAL_REVERIFY,     /* re-run verification on value */
    TARDY_HEAL_RECONSTRUCT,  /* reconstruct from replicas */
} tardy_heal_action_t;

/* Attempt to heal an agent. Returns 0 if healed, -1 if unrecoverable. */
int tardy_heal(void *vm_ptr, tardy_uuid_t agent_id,
                tardy_heal_action_t action);

#endif
