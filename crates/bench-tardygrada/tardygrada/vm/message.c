/*
 * Tardygrada VM — Message Queue Implementation
 * Circular buffer for agent-to-agent communication.
 */

#include "message.h"
#include <string.h>

void tardy_mq_init(tardy_message_queue_t *q)
{
    if (!q)
        return;
    memset(q, 0, sizeof(tardy_message_queue_t));
}

int tardy_mq_push(tardy_message_queue_t *q, const tardy_message_t *msg)
{
    if (!q || !msg)
        return -1;

    if (q->count >= TARDY_MSG_QUEUE_SIZE)
        return -1; /* queue full */

    q->messages[q->tail] = *msg;
    q->tail = (q->tail + 1) % TARDY_MSG_QUEUE_SIZE;
    q->count++;
    return 0;
}

int tardy_mq_pop(tardy_message_queue_t *q, tardy_message_t *out)
{
    if (!q || !out)
        return -1;

    if (q->count <= 0)
        return -1; /* queue empty */

    *out = q->messages[q->head];
    q->head = (q->head + 1) % TARDY_MSG_QUEUE_SIZE;
    q->count--;
    return 0;
}

int tardy_mq_count(const tardy_message_queue_t *q)
{
    if (!q)
        return 0;
    return q->count;
}
