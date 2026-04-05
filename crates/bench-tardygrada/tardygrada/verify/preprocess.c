/*
 * Tardygrada -- Text Preprocessor Implementation
 *
 * The subagent decomposer. Strips LLM formatting, extracts structured
 * claims, then feeds clean text to the rule-based decomposer.
 */

#include "preprocess.h"
#include "decompose.h"
#include <string.h>
#include <ctype.h>
#include <stdio.h>

/* ============================================
 * Markdown Stripping
 * ============================================ */

int tardy_strip_markdown(char *text, int len)
{
    char *out = text;
    int oi = 0;
    int i = 0;

    while (i < len) {
        /* Skip ## headers at start of line */
        if ((i == 0 || text[i - 1] == '\n') && text[i] == '#') {
            while (i < len && text[i] == '#') i++;
            while (i < len && text[i] == ' ') i++;
            continue;
        }

        /* Skip **bold** markers */
        if (i + 1 < len && text[i] == '*' && text[i + 1] == '*') {
            i += 2;
            continue;
        }

        /* Skip single * at start of line (bullet) */
        if ((i == 0 || text[i - 1] == '\n') && text[i] == '*' && i + 1 < len && text[i + 1] == ' ') {
            i += 2;
            continue;
        }

        /* Skip - at start of line (bullet) */
        if ((i == 0 || text[i - 1] == '\n') && text[i] == '-' && i + 1 < len && text[i + 1] == ' ') {
            i += 2;
            continue;
        }

        /* Convert [text](url) links to just text */
        if (text[i] == '[') {
            int j = i + 1;
            while (j < len && text[j] != ']') j++;
            if (j < len && j + 1 < len && text[j + 1] == '(') {
                /* Copy link text */
                for (int k = i + 1; k < j && oi < len - 1; k++)
                    out[oi++] = text[k];
                /* Skip ](url) */
                j += 2;
                while (j < len && text[j] != ')') j++;
                if (j < len) j++;
                i = j;
                continue;
            }
        }

        /* Skip backtick code markers */
        if (text[i] == '`') {
            i++;
            continue;
        }

        /* Convert newlines to periods (sentence boundaries) */
        if (text[i] == '\n') {
            /* Only if previous char isn't already a sentence ender */
            if (oi > 0 && out[oi - 1] != '.' && out[oi - 1] != '!' &&
                out[oi - 1] != '?' && out[oi - 1] != '\n') {
                out[oi++] = '.';
                out[oi++] = ' ';
            } else if (oi > 0) {
                out[oi++] = ' ';
            }
            i++;
            continue;
        }

        out[oi++] = text[i++];
    }

    out[oi] = '\0';
    return oi;
}

/* ============================================
 * Key-Value Extraction
 *
 * LLMs love to output:
 *   **P/E Ratio:** 60.5
 *   Stock Price Range: $180-$270
 *   - Revenue: $96 billion
 *
 * These are structured facts hiding in plain text.
 * ============================================ */

static int is_kv_separator(const char *text, int pos, int len)
{
    /* "Key: Value" or "Key - Value" */
    if (text[pos] == ':' && pos + 1 < len && text[pos + 1] == ' ')
        return 2; /* skip ": " */

    return 0;
}

/* Extract subject from first sentence or use default */
static void find_subject(const char *text, int len, char *out, int max)
{
    /* Look for first capitalized word or proper noun */
    int i = 0;
    /* Skip markdown/whitespace */
    while (i < len && (text[i] == '#' || text[i] == '*' || text[i] == ' '))
        i++;

    int j = 0;
    /* Copy until period, newline, or colon */
    while (i < len && text[i] != '.' && text[i] != '\n' &&
           text[i] != ':' && j < max - 1) {
        out[j++] = text[i++];
    }
    out[j] = '\0';

    /* Trim */
    while (j > 0 && (out[j - 1] == ' ' || out[j - 1] == '*'))
        out[--j] = '\0';

    /* If too long or empty, use generic */
    if (j > 40 || j == 0)
        strncpy(out, "subject", max);
}

int tardy_extract_keyvalue(const char *text, int len,
                            const char *subject,
                            tardy_triple_t *triples, int max_triples)
{
    int count = 0;
    int i = 0;

    while (i < len && count < max_triples) {
        /* Find start of a line */
        while (i < len && (text[i] == ' ' || text[i] == '\t' ||
                           text[i] == '-' || text[i] == '*' ||
                           text[i] == '#'))
            i++;

        if (i >= len) break;

        /* Look for "Key: Value" pattern on this line */
        int line_start = i;
        int colon_pos = -1;

        while (i < len && text[i] != '\n') {
            if (colon_pos < 0) {
                int sep = is_kv_separator(text, i, len);
                if (sep > 0 && i > line_start + 1) {
                    colon_pos = i;
                }
            }
            i++;
        }

        if (colon_pos > 0) {
            /* Extract key */
            char key[TARDY_MAX_TRIPLE_LEN];
            int klen = colon_pos - line_start;
            if (klen > TARDY_MAX_TRIPLE_LEN - 1) klen = TARDY_MAX_TRIPLE_LEN - 1;
            memcpy(key, text + line_start, klen);
            key[klen] = '\0';

            /* Strip markdown from key */
            char *k = key;
            while (*k == '*' || *k == ' ') k++;
            int kend = (int)strlen(k) - 1;
            while (kend >= 0 && (k[kend] == '*' || k[kend] == ' '))
                k[kend--] = '\0';

            /* Extract value */
            char val[TARDY_MAX_TRIPLE_LEN];
            int vstart = colon_pos + 2; /* skip ": " */
            int vlen = i - vstart;
            if (vlen > TARDY_MAX_TRIPLE_LEN - 1) vlen = TARDY_MAX_TRIPLE_LEN - 1;
            if (vlen > 0) {
                memcpy(val, text + vstart, vlen);
                val[vlen] = '\0';

                /* Strip trailing whitespace */
                int ve = vlen - 1;
                while (ve >= 0 && (val[ve] == ' ' || val[ve] == '\n'))
                    val[ve--] = '\0';
            } else {
                val[0] = '\0';
            }

            /* Create triple if both key and value are non-empty */
            if (k[0] && val[0] && strlen(k) > 1 && strlen(val) > 1) {
                strncpy(triples[count].subject, subject,
                        TARDY_MAX_TRIPLE_LEN - 1);

                /* Normalize key to predicate format (lowercase, underscores) */
                char pred[TARDY_MAX_TRIPLE_LEN];
                int pi = 0;
                for (int ki = 0; k[ki] && pi < TARDY_MAX_TRIPLE_LEN - 1; ki++) {
                    char c = k[ki];
                    if (c == ' ' || c == '/' || c == '-')
                        pred[pi++] = '_';
                    else if (c >= 'A' && c <= 'Z')
                        pred[pi++] = c + 32;
                    else if ((c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c == '_')
                        pred[pi++] = c;
                }
                pred[pi] = '\0';

                strncpy(triples[count].predicate, pred,
                        TARDY_MAX_TRIPLE_LEN - 1);
                strncpy(triples[count].object, val,
                        TARDY_MAX_TRIPLE_LEN - 1);

                count++;
            }
        }

        /* Move to next line */
        if (i < len) i++;
    }

    return count;
}

/* ============================================
 * Full Preprocessing Pipeline
 *
 * 1. Strip markdown
 * 2. Extract key-value pairs
 * 3. Run rule-based decomposer on cleaned text
 * 4. Merge and deduplicate
 *
 * This is the "subagent decomposer" -- deterministic,
 * no API calls, handles LLM output formatting.
 * ============================================ */

int tardy_preprocess_and_decompose(const char *text, int len,
                                    tardy_triple_t *triples, int max_triples)
{
    if (!text || len <= 0 || !triples || max_triples <= 0)
        return 0;

    int count = 0;

    /* Step 1: Find subject from first line */
    char subject[TARDY_MAX_TRIPLE_LEN];
    find_subject(text, len, subject, TARDY_MAX_TRIPLE_LEN);

    /* Step 2: Extract key-value pairs from original text */
    count += tardy_extract_keyvalue(text, len, subject,
                                     triples + count, max_triples - count);

    /* Step 3: Strip markdown and run rule-based decomposer */
    char cleaned[4096];
    int clen = len < (int)sizeof(cleaned) - 1 ? len : (int)sizeof(cleaned) - 1;
    memcpy(cleaned, text, clen);
    cleaned[clen] = '\0';
    clen = tardy_strip_markdown(cleaned, clen);

    if (clen > 0 && count < max_triples) {
        count += tardy_decompose(cleaned, clen,
                                  triples + count, max_triples - count);
    }

    /* Step 4: Deduplicate */
    for (int i = 0; i < count; i++) {
        for (int j = i + 1; j < count; j++) {
            if (strcmp(triples[i].subject, triples[j].subject) == 0 &&
                strcmp(triples[i].predicate, triples[j].predicate) == 0 &&
                strcmp(triples[i].object, triples[j].object) == 0) {
                memmove(&triples[j], &triples[j + 1],
                        sizeof(tardy_triple_t) * (count - j - 1));
                count--;
                j--;
            }
        }
    }

    return count;
}
