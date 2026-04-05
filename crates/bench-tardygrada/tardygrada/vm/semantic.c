/*
 * Tardygrada VM — Semantic Query Implementation
 * Keyword-based search: tokenize query, match against agent names and values.
 * Score = (matching words) / (total query words).
 */

#include "semantic.h"
#include "vm.h"
#include <string.h>

/* ============================================
 * Helpers
 * ============================================ */

static char to_lower(char c)
{
    if (c >= 'A' && c <= 'Z')
        return (char)(c + ('a' - 'A'));
    return c;
}

/* Case-insensitive substring search */
static int ci_strstr(const char *haystack, const char *needle, int needle_len)
{
    if (!haystack || !needle || needle_len <= 0)
        return 0;

    int hlen = (int)strlen(haystack);
    for (int i = 0; i <= hlen - needle_len; i++) {
        int match = 1;
        for (int j = 0; j < needle_len; j++) {
            if (to_lower(haystack[i + j]) != to_lower(needle[j])) {
                match = 0;
                break;
            }
        }
        if (match)
            return 1;
    }
    return 0;
}

/* ============================================
 * Tokenizer — split query on spaces
 * ============================================ */

#define MAX_QUERY_WORDS 32

typedef struct {
    const char *start;
    int         len;
} query_word_t;

static int tokenize_query(const char *query, query_word_t *words, int max_words)
{
    int count = 0;
    const char *p = query;

    while (*p && count < max_words) {
        /* skip spaces */
        while (*p == ' ')
            p++;
        if (*p == '\0')
            break;

        /* start of word */
        words[count].start = p;
        int len = 0;
        while (*p && *p != ' ') {
            p++;
            len++;
        }
        words[count].len = len;
        count++;
    }
    return count;
}

/* ============================================
 * Semantic Query
 * ============================================ */

int tardy_vm_query(void *vm_ptr, tardy_uuid_t scope,
                    const char *query,
                    tardy_query_result_t *results, int max_results)
{
    tardy_vm_t *vm = (tardy_vm_t *)vm_ptr;

    if (!vm || !query || !results || max_results <= 0)
        return 0;

    /* Cap max_results */
    if (max_results > TARDY_MAX_QUERY_RESULTS)
        max_results = TARDY_MAX_QUERY_RESULTS;

    /* Tokenize query */
    query_word_t words[MAX_QUERY_WORDS];
    int word_count = tokenize_query(query, words, MAX_QUERY_WORDS);
    if (word_count == 0)
        return 0;

    /* Find the scope agent */
    tardy_agent_t *scope_agent = tardy_vm_find(vm, scope);
    if (!scope_agent)
        return 0;

    /* Score each child in scope */
    int result_count = 0;

    for (int i = 0; i < scope_agent->context.child_count; i++) {
        tardy_named_child_t *child = &scope_agent->context.children[i];

        /* Find the child agent */
        tardy_agent_t *agent = tardy_vm_find(vm, child->agent_id);
        if (!agent || agent->state == TARDY_STATE_DEAD)
            continue;

        /* Count matching words */
        int matches = 0;
        for (int w = 0; w < word_count; w++) {
            int found = 0;

            /* Check agent name */
            if (ci_strstr(child->name, words[w].start, words[w].len))
                found = 1;

            /* Check string value if agent holds a string */
            if (!found && agent->type_tag == TARDY_TYPE_STR) {
                char val_buf[256];
                memset(val_buf, 0, sizeof(val_buf));
                tardy_page_read(&agent->memory.primary, val_buf,
                                sizeof(val_buf) - 1);
                if (ci_strstr(val_buf, words[w].start, words[w].len))
                    found = 1;
            }

            /* Check error value (also string-like) */
            if (!found && agent->type_tag == TARDY_TYPE_ERROR) {
                char val_buf[256];
                memset(val_buf, 0, sizeof(val_buf));
                tardy_page_read(&agent->memory.primary, val_buf,
                                sizeof(val_buf) - 1);
                if (ci_strstr(val_buf, words[w].start, words[w].len))
                    found = 1;
            }

            if (found)
                matches++;
        }

        if (matches == 0)
            continue;

        float score = (float)matches / (float)word_count;

        /* Insert into results, maintaining descending score order */
        int insert_pos = result_count;
        for (int r = 0; r < result_count; r++) {
            if (score > results[r].score) {
                insert_pos = r;
                break;
            }
        }

        if (insert_pos >= max_results)
            continue;

        /* Shift down to make room */
        int shift_end = result_count < max_results ? result_count : max_results - 1;
        for (int r = shift_end; r > insert_pos; r--) {
            results[r] = results[r - 1];
        }

        /* Insert */
        results[insert_pos].agent_id = child->agent_id;
        results[insert_pos].score = score;
        strncpy(results[insert_pos].name, child->name,
                sizeof(results[insert_pos].name) - 1);
        results[insert_pos].name[sizeof(results[insert_pos].name) - 1] = '\0';

        if (result_count < max_results)
            result_count++;
    }

    return result_count;
}
