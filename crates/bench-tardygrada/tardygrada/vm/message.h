/*
 * Tardygrada VM — Agent-to-Agent Messaging
 * Agents communicate internally via message queues.
 * Every message carries provenance: who sent it, when, with integrity hash.
 */

#ifndef TARDY_MESSAGE_H
#define TARDY_MESSAGE_H

#include "types.h"
#include "crypto.h"

#define TARDY_MAX_PAYLOAD    512
#define TARDY_MSG_QUEUE_SIZE 64

typedef struct {
    tardy_uuid_t      from;
    tardy_uuid_t      to;
    tardy_type_t      payload_type;
    char              payload[TARDY_MAX_PAYLOAD];
    size_t            payload_len;
    tardy_hash_t      hash;          /* hash of payload for integrity */
    tardy_timestamp_t sent_at;
} tardy_message_t;

typedef struct {
    tardy_message_t messages[TARDY_MSG_QUEUE_SIZE];
    int             head;
    int             tail;
    int             count;
} tardy_message_queue_t;

/* Initialize a message queue */
void tardy_mq_init(tardy_message_queue_t *q);

/* Enqueue a message. Returns 0 on success, -1 if full. */
int tardy_mq_push(tardy_message_queue_t *q, const tardy_message_t *msg);

/* Dequeue a message. Returns 0 on success, -1 if empty. */
int tardy_mq_pop(tardy_message_queue_t *q, tardy_message_t *out);

/* Check if queue has messages */
int tardy_mq_count(const tardy_message_queue_t *q);

#endif /* TARDY_MESSAGE_H */
