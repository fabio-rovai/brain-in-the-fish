/*
 * Tardygrada VM — Sovereign Disk Persistence
 * @sovereign agents never die and never demote — but they dump
 * to disk when idle to bound memory usage. When accessed again,
 * they're loaded back and verified.
 */

#ifndef TARDY_PERSIST_H
#define TARDY_PERSIST_H

#include "types.h"
#include "crypto.h"

#define TARDY_PERSIST_DIR "/tmp/tardygrada-persist"

/* Dump a sovereign agent's value to disk.
 * File: <dir>/<agent-uuid-hex>.dat
 * Format: [hash:32][data_size:8][data:N]
 */
int tardy_persist_dump(const void *agent_ptr, const char *dir);

/* Load a sovereign agent's value from disk and verify hash.
 * Returns 0 on success, -1 on failure or hash mismatch. */
int tardy_persist_load(void *agent_ptr, const char *dir);

/* Check if a persistent file exists for an agent */
int tardy_persist_exists(tardy_uuid_t id, const char *dir);

#endif /* TARDY_PERSIST_H */
