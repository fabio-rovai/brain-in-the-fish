/*
 * Tardygrada — Terraform Protocol Implementation
 *
 * Scans agentic repos, extracts patterns, generates .tardy files.
 * Every discovery is treated as a claim that can be verified.
 */

#include "terraform.h"
#include <string.h>
#include <stdio.h>
#include <dirent.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>

/* ============================================
 * Framework Detection
 * ============================================ */

static int file_contains(const char *path, const char *needle)
{
    int fd = open(path, O_RDONLY);
    if (fd < 0) return 0;

    char buf[8192];
    ssize_t n = read(fd, buf, sizeof(buf) - 1);
    close(fd);
    if (n <= 0) return 0;
    buf[n] = '\0';

    return strstr(buf, needle) != NULL;
}

static int check_requirements(const char *repo, const char *package)
{
    char path[512];
    /* Check requirements.txt */
    snprintf(path, sizeof(path), "%s/requirements.txt", repo);
    if (file_contains(path, package)) return 1;
    /* Check pyproject.toml */
    snprintf(path, sizeof(path), "%s/pyproject.toml", repo);
    if (file_contains(path, package)) return 1;
    /* Check setup.py */
    snprintf(path, sizeof(path), "%s/setup.py", repo);
    if (file_contains(path, package)) return 1;
    return 0;
}

tardy_tf_framework_t tardy_tf_detect_framework(const char *repo_path)
{
    if (check_requirements(repo_path, "crewai"))    return TARDY_TF_CREWAI;
    if (check_requirements(repo_path, "autogen"))    return TARDY_TF_AUTOGEN;
    if (check_requirements(repo_path, "pyautogen"))  return TARDY_TF_AUTOGEN;
    if (check_requirements(repo_path, "langgraph"))  return TARDY_TF_LANGGRAPH;
    if (check_requirements(repo_path, "langchain"))  return TARDY_TF_LANGCHAIN;
    if (check_requirements(repo_path, "llama-index")) return TARDY_TF_LLAMAINDEX;
    if (check_requirements(repo_path, "llama_index")) return TARDY_TF_LLAMAINDEX;

    /* Check source files for imports */
    char path[512];
    snprintf(path, sizeof(path), "%s/src", repo_path);

    DIR *dir = opendir(path);
    if (!dir) {
        snprintf(path, sizeof(path), "%s", repo_path);
        dir = opendir(path);
    }
    if (!dir) return TARDY_TF_UNKNOWN;
    closedir(dir);

    return TARDY_TF_GENERIC;
}

/* ============================================
 * Python Pattern Scanner
 *
 * Extracts agents, tools, tasks from Python files.
 * Pattern matching — not parsing. Good enough for terraform.
 * ============================================ */

/* Extract value from pattern: key="value" or key='value' */
static int extract_kwarg(const char *line, const char *key,
                          char *out, int max_len)
{
    char pattern[128];
    snprintf(pattern, sizeof(pattern), "%s=", key);
    const char *pos = strstr(line, pattern);
    if (!pos) return 0;

    pos += strlen(pattern);
    /* Skip whitespace */
    while (*pos == ' ') pos++;

    char quote = *pos;
    if (quote != '"' && quote != '\'') return 0;
    pos++;

    int i = 0;
    while (pos[i] && pos[i] != quote && i < max_len - 1) {
        out[i] = pos[i];
        i++;
    }
    out[i] = '\0';
    return i > 0;
}

/* Scan a single Python file */
static void scan_py_file(const char *path, tardy_tf_analysis_t *out)
{
    int fd = open(path, O_RDONLY);
    if (fd < 0) return;

    char buf[65536];
    ssize_t n = read(fd, buf, sizeof(buf) - 1);
    close(fd);
    if (n <= 0) return;
    buf[n] = '\0';

    out->total_lines += 1; /* approximate */
    for (ssize_t i = 0; i < n; i++)
        if (buf[i] == '\n') out->total_lines++;

    /* Scan line by line */
    char *line = buf;
    while (line && *line) {
        char *next = strchr(line, '\n');
        if (next) *next = '\0';

        /* CrewAI: Agent(role="...", goal="...") */
        if (strstr(line, "Agent(") && out->agent_count < TARDY_TF_MAX_AGENTS) {
            tardy_tf_agent_t *a = &out->agents[out->agent_count];
            memset(a, 0, sizeof(tardy_tf_agent_t));
            extract_kwarg(line, "role", a->role, TARDY_TF_MAX_DESC);
            extract_kwarg(line, "goal", a->goal, TARDY_TF_MAX_DESC);
            /* Try to get name from variable assignment */
            char *eq = strchr(line, '=');
            if (eq && eq > line) {
                int nlen = (int)(eq - line);
                while (nlen > 0 && line[nlen - 1] == ' ') nlen--;
                if (nlen > 0 && nlen < TARDY_TF_MAX_NAME) {
                    memcpy(a->name, line, nlen);
                    a->name[nlen] = '\0';
                    /* Trim leading whitespace */
                    char *start = a->name;
                    while (*start == ' ' || *start == '\t') start++;
                    if (start != a->name)
                        memmove(a->name, start, strlen(start) + 1);
                }
            }
            if (a->role[0] || a->name[0]) {
                a->verified = 1;
                out->agent_count++;
            }
        }

        /* CrewAI/LangChain: @tool decorator */
        if ((strstr(line, "@tool") || strstr(line, "@tool(")) &&
            out->tool_count < TARDY_TF_MAX_TOOLS) {
            tardy_tf_tool_t *t = &out->tools[out->tool_count];
            memset(t, 0, sizeof(tardy_tf_tool_t));
            /* Tool name is usually on the next line: def tool_name(...) */
            if (next && next[1]) {
                char *def = strstr(next + 1, "def ");
                if (def) {
                    def += 4;
                    int i = 0;
                    while (def[i] && def[i] != '(' && i < TARDY_TF_MAX_NAME - 1) {
                        t->name[i] = def[i];
                        i++;
                    }
                    t->name[i] = '\0';
                }
            }
            t->verified = 1;
            out->tool_count++;
        }

        /* CrewAI: Task(description="...") */
        if (strstr(line, "Task(") && out->task_count < TARDY_TF_MAX_TASKS) {
            tardy_tf_task_t *t = &out->tasks[out->task_count];
            memset(t, 0, sizeof(tardy_tf_task_t));
            extract_kwarg(line, "description", t->description, TARDY_TF_MAX_DESC);
            extract_kwarg(line, "expected_output", t->expected_output, TARDY_TF_MAX_DESC);
            extract_kwarg(line, "agent", t->agent, TARDY_TF_MAX_NAME);
            if (t->description[0]) {
                t->verified = 1;
                out->task_count++;
            }
        }

        /* AutoGen: ConversableAgent / AssistantAgent / UserProxyAgent */
        if ((strstr(line, "ConversableAgent(") ||
             strstr(line, "AssistantAgent(") ||
             strstr(line, "UserProxyAgent(")) &&
            out->agent_count < TARDY_TF_MAX_AGENTS) {
            tardy_tf_agent_t *a = &out->agents[out->agent_count];
            memset(a, 0, sizeof(tardy_tf_agent_t));
            extract_kwarg(line, "name", a->name, TARDY_TF_MAX_NAME);
            extract_kwarg(line, "system_message", a->goal, TARDY_TF_MAX_DESC);
            if (a->name[0]) {
                a->verified = 1;
                if (strstr(line, "AssistantAgent"))
                    strncpy(a->role, "assistant", TARDY_TF_MAX_DESC);
                else if (strstr(line, "UserProxyAgent"))
                    strncpy(a->role, "user_proxy", TARDY_TF_MAX_DESC);
                out->agent_count++;
            }
        }

        /* LangGraph: StateGraph / add_node / add_edge */
        if (strstr(line, "add_node(") && out->agent_count < TARDY_TF_MAX_AGENTS) {
            tardy_tf_agent_t *a = &out->agents[out->agent_count];
            memset(a, 0, sizeof(tardy_tf_agent_t));
            /* Extract node name from first string arg */
            char *quote = strchr(line, '"');
            if (!quote) quote = strchr(line, '\'');
            if (quote) {
                char q = *quote;
                quote++;
                int i = 0;
                while (quote[i] && quote[i] != q && i < TARDY_TF_MAX_NAME - 1) {
                    a->name[i] = quote[i];
                    i++;
                }
                a->name[i] = '\0';
                strncpy(a->role, "graph_node", TARDY_TF_MAX_DESC);
                a->verified = 1;
                out->agent_count++;
            }
        }

        /* LlamaIndex: VectorStoreIndex / QueryEngine */
        if (strstr(line, "VectorStoreIndex") || strstr(line, "query_engine")) {
            if (out->tool_count < TARDY_TF_MAX_TOOLS) {
                tardy_tf_tool_t *t = &out->tools[out->tool_count];
                memset(t, 0, sizeof(tardy_tf_tool_t));
                strncpy(t->name, "query_engine", TARDY_TF_MAX_NAME);
                strncpy(t->description, "RAG query over indexed documents",
                        TARDY_TF_MAX_DESC);
                t->verified = 1;
                out->tool_count++;
            }
        }

        /* Orchestration patterns */
        if (strstr(line, "Crew(") || strstr(line, "crew.kickoff"))
            strncpy(out->orchestration, "sequential", TARDY_TF_MAX_DESC);
        if (strstr(line, "GroupChat(") || strstr(line, "group_chat"))
            strncpy(out->orchestration, "group_chat", TARDY_TF_MAX_DESC);
        if (strstr(line, "StateGraph("))
            strncpy(out->orchestration, "state_graph", TARDY_TF_MAX_DESC);

        line = next ? next + 1 : NULL;
    }
}

/* Recursively scan directory for .py files */
static void scan_dir(const char *dir_path, tardy_tf_analysis_t *out, int depth)
{
    if (depth > 5) return; /* max recursion */

    DIR *dir = opendir(dir_path);
    if (!dir) return;

    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL) {
        if (entry->d_name[0] == '.') continue;
        if (strcmp(entry->d_name, "node_modules") == 0) continue;
        if (strcmp(entry->d_name, "__pycache__") == 0) continue;
        if (strcmp(entry->d_name, ".git") == 0) continue;
        if (strcmp(entry->d_name, "venv") == 0) continue;
        if (strcmp(entry->d_name, ".venv") == 0) continue;

        char path[1024];
        snprintf(path, sizeof(path), "%s/%s", dir_path, entry->d_name);

        struct stat st;
        if (stat(path, &st) != 0) continue;

        if (S_ISDIR(st.st_mode)) {
            scan_dir(path, out, depth + 1);
        } else if (S_ISREG(st.st_mode)) {
            out->total_files++;
            int nlen = (int)strlen(entry->d_name);
            if (nlen > 3 && strcmp(entry->d_name + nlen - 3, ".py") == 0) {
                scan_py_file(path, out);
            }
            /* Count lines for non-Python files too (TS, JS, etc.) */
            if ((nlen > 3 && strcmp(entry->d_name + nlen - 3, ".ts") == 0) ||
                (nlen > 3 && strcmp(entry->d_name + nlen - 3, ".js") == 0) ||
                (nlen > 4 && strcmp(entry->d_name + nlen - 4, ".tsx") == 0) ||
                (nlen > 4 && strcmp(entry->d_name + nlen - 4, ".jsx") == 0)) {
                int fd = open(path, O_RDONLY);
                if (fd >= 0) {
                    char buf[8192];
                    ssize_t n;
                    while ((n = read(fd, buf, sizeof(buf))) > 0) {
                        for (ssize_t bi = 0; bi < n; bi++)
                            if (buf[bi] == '\n') out->total_lines++;
                    }
                    close(fd);
                }
            }
        }
    }
    closedir(dir);
}

/* ============================================
 * Analyze
 * ============================================ */

int tardy_tf_analyze(const char *repo_path, tardy_tf_analysis_t *out)
{
    if (!repo_path || !out) return -1;
    memset(out, 0, sizeof(tardy_tf_analysis_t));

    /* Detect framework */
    out->framework = tardy_tf_detect_framework(repo_path);

    /* Extract repo name from path and sanitize */
    const char *name = repo_path;
    const char *slash = strrchr(repo_path, '/');
    if (slash) name = slash + 1;
    strncpy(out->repo_name, name, TARDY_TF_MAX_NAME - 1);
    /* Strip common suffixes like -test, _test */
    {
        int rlen = (int)strlen(out->repo_name);
        if (rlen > 5 && strcmp(out->repo_name + rlen - 5, "_test") == 0)
            out->repo_name[rlen - 5] = '\0';
        if (rlen > 5 && strcmp(out->repo_name + rlen - 5, "-test") == 0)
            out->repo_name[rlen - 5] = '\0';
    }
    /* Replace non-alphanumeric chars with underscore */
    for (int i = 0; out->repo_name[i]; i++) {
        char c = out->repo_name[i];
        if (!((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
              (c >= '0' && c <= '9') || c == '_'))
            out->repo_name[i] = '_';
    }

    /* Scan Python files */
    scan_dir(repo_path, out, 0);

    /* Count deps from requirements.txt */
    char req_path[512];
    snprintf(req_path, sizeof(req_path), "%s/requirements.txt", repo_path);
    int fd = open(req_path, O_RDONLY);
    if (fd >= 0) {
        char buf[8192];
        ssize_t n = read(fd, buf, sizeof(buf) - 1);
        close(fd);
        if (n > 0) {
            buf[n] = '\0';
            for (ssize_t i = 0; i < n; i++)
                if (buf[i] == '\n') out->total_deps++;
        }
    }

    /* Post-process: sanitize names, deduplicate agents */
    {
        /* Sanitize agent names */
        for (int i = 0; i < out->agent_count; i++) {
            char *n = out->agents[i].name;
            /* Skip invalid names (contain brackets, parens, spaces, keywords) */
            if (strchr(n, '[') || strchr(n, '(') || strchr(n, ' ') ||
                strstr(n, "return") || strstr(n, "kwargs") ||
                n[0] == '\0') {
                /* Remove this agent */
                memmove(&out->agents[i], &out->agents[i + 1],
                        sizeof(tardy_tf_agent_t) * (out->agent_count - i - 1));
                out->agent_count--;
                i--;
                continue;
            }
            /* Replace non-identifier chars */
            for (int j = 0; n[j]; j++) {
                char c = n[j];
                if (!((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
                      (c >= '0' && c <= '9') || c == '_'))
                    n[j] = '_';
            }
        }

        /* Deduplicate agents by name */
        for (int i = 0; i < out->agent_count; i++) {
            for (int j = i + 1; j < out->agent_count; j++) {
                if (strcmp(out->agents[i].name, out->agents[j].name) == 0) {
                    memmove(&out->agents[j], &out->agents[j + 1],
                            sizeof(tardy_tf_agent_t) * (out->agent_count - j - 1));
                    out->agent_count--;
                    j--;
                }
            }
        }

        /* Deduplicate tools by name */
        for (int i = 0; i < out->tool_count; i++) {
            for (int j = i + 1; j < out->tool_count; j++) {
                if (strcmp(out->tools[i].name, out->tools[j].name) == 0) {
                    memmove(&out->tools[j], &out->tools[j + 1],
                            sizeof(tardy_tf_tool_t) * (out->tool_count - j - 1));
                    out->tool_count--;
                    j--;
                }
            }
        }

        /* Deduplicate tasks by description */
        for (int i = 0; i < out->task_count; i++) {
            for (int j = i + 1; j < out->task_count; j++) {
                if (strcmp(out->tasks[i].description, out->tasks[j].description) == 0) {
                    memmove(&out->tasks[j], &out->tasks[j + 1],
                            sizeof(tardy_tf_task_t) * (out->task_count - j - 1));
                    out->task_count--;
                    j--;
                }
            }
        }
    }

    return 0;
}

int tardy_tf_scan_python(const char *path, tardy_tf_analysis_t *out)
{
    scan_py_file(path, out);
    return 0;
}

/* ============================================
 * Generate .tardy File
 * ============================================ */

int tardy_tf_generate(const tardy_tf_analysis_t *analysis,
                       char *output, int max_len)
{
    if (!analysis || !output) return -1;

    int w = 0;

    /* Header comment */
    w += snprintf(output + w, max_len - w,
        "// Tardygrada terraform of: %s\n"
        "// Original: %d files, %d lines, %d dependencies\n"
        "// Framework: %s\n"
        "// Generated by: tardy terraform\n"
        "//\n"
        "// This file replaces the entire framework with verified agents.\n"
        "// Every output goes through 8-layer verification + BFT consensus.\n"
        "\n",
        analysis->repo_name,
        analysis->total_files,
        analysis->total_lines,
        analysis->total_deps,
        analysis->framework == TARDY_TF_CREWAI ? "CrewAI" :
        analysis->framework == TARDY_TF_AUTOGEN ? "AutoGen" :
        analysis->framework == TARDY_TF_LANGCHAIN ? "LangChain" :
        analysis->framework == TARDY_TF_LANGGRAPH ? "LangGraph" :
        analysis->framework == TARDY_TF_LLAMAINDEX ? "LlamaIndex" :
        "generic");

    /* Main agent */
    w += snprintf(output + w, max_len - w,
        "agent %s @sovereign @semantics(\n"
        "    truth.min_confidence: 0.90,\n"
        "    truth.min_consensus_agents: 3,\n"
        ") {\n",
        analysis->repo_name);

    /* Constitution */
    w += snprintf(output + w, max_len - w,
        "    invariant(trust_min: @verified)\n"
        "    invariant(non_empty)\n\n");

    /* Agents become receive() slots */
    if (analysis->agent_count > 0) {
        w += snprintf(output + w, max_len - w,
            "    // Agents (%d found in original repo)\n",
            analysis->agent_count);

        /* Track used names to avoid duplicates */
        char used_names[TARDY_TF_MAX_AGENTS][TARDY_TF_MAX_NAME];
        int used_count = 0;

        for (int i = 0; i < analysis->agent_count; i++) {
            const tardy_tf_agent_t *a = &analysis->agents[i];
            char name[TARDY_TF_MAX_NAME];
            const char *src = a->name[0] ? a->name : "worker";
            strncpy(name, src, TARDY_TF_MAX_NAME - 4);
            name[TARDY_TF_MAX_NAME - 4] = '\0';

            /* Avoid Tardygrada reserved keywords */
            if (strcmp(name, "agent") == 0 || strcmp(name, "let") == 0 ||
                strcmp(name, "fn") == 0 || strcmp(name, "fork") == 0 ||
                strcmp(name, "freeze") == 0 || strcmp(name, "receive") == 0 ||
                strcmp(name, "coordinate") == 0 || strcmp(name, "invariant") == 0) {
                char prefixed[TARDY_TF_MAX_NAME];
                snprintf(prefixed, sizeof(prefixed), "_%s", name);
                strncpy(name, prefixed, TARDY_TF_MAX_NAME - 1);
            }

            /* Check for duplicate and add suffix */
            int dup = 0;
            for (int j = 0; j < used_count; j++) {
                if (strcmp(used_names[j], name) == 0) { dup = 1; break; }
            }
            if (dup) {
                char suffixed[TARDY_TF_MAX_NAME];
                snprintf(suffixed, sizeof(suffixed), "%s_%d", name, i);
                strncpy(name, suffixed, TARDY_TF_MAX_NAME - 1);
            }
            strncpy(used_names[used_count++], name, TARDY_TF_MAX_NAME);

            const char *desc = a->role[0] ? a->role : a->goal;
            w += snprintf(output + w, max_len - w,
                "    let %s: Fact = receive(\"%.80s\") grounded_in(domain) @verified\n",
                name, desc[0] ? desc : name);
        }
        w += snprintf(output + w, max_len - w, "\n");
    }

    /* Tasks become receive() slots */
    if (analysis->task_count > 0) {
        w += snprintf(output + w, max_len - w,
            "    // Tasks (%d found in original repo)\n",
            analysis->task_count);

        for (int i = 0; i < analysis->task_count; i++) {
            const tardy_tf_task_t *t = &analysis->tasks[i];
            w += snprintf(output + w, max_len - w,
                "    let task_%d: Fact = receive(\"%.80s\") grounded_in(spec) @verified\n",
                i, t->description);
        }
        w += snprintf(output + w, max_len - w, "\n");
    }

    /* Tools become exec() agents */
    if (analysis->tool_count > 0) {
        w += snprintf(output + w, max_len - w,
            "    // Tools (%d found in original repo)\n",
            analysis->tool_count);

        for (int i = 0; i < analysis->tool_count; i++) {
            const tardy_tf_tool_t *t = &analysis->tools[i];
            if (!t->name[0]) continue;

            /* Sanitize tool name */
            char tname[TARDY_TF_MAX_NAME];
            strncpy(tname, t->name, TARDY_TF_MAX_NAME - 1);
            tname[TARDY_TF_MAX_NAME - 1] = '\0';
            for (int j = 0; tname[j]; j++) {
                char c = tname[j];
                if (!((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
                      (c >= '0' && c <= '9') || c == '_'))
                    tname[j] = '_';
            }

            /* Map tool name to a real shell command */
            const char *cmd = NULL;
            /* Web / HTTP */
            if (strstr(t->name, "search") || strstr(t->name, "Search"))
                cmd = "curl -s 'https://api.duckduckgo.com/?q=${QUERY:-test}&format=json' | head -c 2000";
            else if (strstr(t->name, "scrape") || strstr(t->name, "Scrape") ||
                     strstr(t->name, "crawl") || strstr(t->name, "Crawl") ||
                     strstr(t->name, "go_to") || strstr(t->name, "webpage"))
                cmd = "curl -s '${URL:-https://example.com}' | head -c 4000";
            else if (strstr(t->name, "get_text") || strstr(t->name, "extract"))
                cmd = "curl -s '${URL:-https://example.com}' | sed 's/<[^>]*>//g' | head -c 2000";
            /* Screenshot / visual */
            else if (strstr(t->name, "screenshot") || strstr(t->name, "Screenshot"))
                cmd = "which screencapture >/dev/null && screencapture -x /tmp/tardy_screenshot.png && echo '/tmp/tardy_screenshot.png' || echo 'screenshot not available'";
            else if (strstr(t->name, "blur") || strstr(t->name, "unblur"))
                cmd = "echo 'visual operation: ${TOOL_NAME}'";
            /* File I/O */
            else if (strstr(t->name, "read") || strstr(t->name, "Read") ||
                     strstr(t->name, "upload") || strstr(t->name, "Upload"))
                cmd = "cat '${FILE:-input.txt}' 2>/dev/null || echo 'file not found'";
            else if (strstr(t->name, "write") || strstr(t->name, "Write") ||
                     strstr(t->name, "save") || strstr(t->name, "Save"))
                cmd = "echo '${CONTENT:-data}' > '${FILE:-output.txt}' && echo 'written to ${FILE:-output.txt}'";
            /* Database / query */
            else if (strstr(t->name, "query") || strstr(t->name, "Query"))
                cmd = "sqlite3 '${DB:-knowledge.db}' '${SQL:-SELECT * FROM facts LIMIT 10}' 2>/dev/null || echo 'no db'";
            /* Form / input */
            else if (strstr(t->name, "fill") || strstr(t->name, "click") ||
                     strstr(t->name, "submit"))
                cmd = "echo 'form action: ${ACTION:-submit} field=${FIELD:-input} value=${VALUE:-data}'";
            /* Email */
            else if (strstr(t->name, "email") || strstr(t->name, "Email") ||
                     strstr(t->name, "mail"))
                cmd = "echo 'email interface: use msmtp or sendmail for real delivery'";
            /* List / directory */
            else if (strstr(t->name, "list") || strstr(t->name, "List"))
                cmd = "ls -la '${DIR:-.}' 2>/dev/null | head -20";
            /* Parse / transform */
            else if (strstr(t->name, "parse") || strstr(t->name, "Parse"))
                cmd = "cat '${FILE:-input.json}' 2>/dev/null | python3 -m json.tool 2>/dev/null || cat '${FILE:-input.json}'";
            /* 2FA / auth */
            else if (strstr(t->name, "2fa") || strstr(t->name, "otp") ||
                     strstr(t->name, "auth"))
                cmd = "echo 'auth/2fa: requires manual intervention or op CLI (1Password)'";
            /* Custom / action */
            else if (strstr(t->name, "custom") || strstr(t->name, "action") ||
                     strstr(t->name, "execute") || strstr(t->name, "run"))
                cmd = "sh -c '${COMMAND:-echo no command specified}'";
            /* Done / completion */
            else if (strstr(t->name, "done") || strstr(t->name, "complete") ||
                     strstr(t->name, "finish"))
                cmd = "echo 'task completed at '$(date -u +%%Y-%%m-%%dT%%H:%%M:%%SZ)";
            /* Fallback: receive() instead of fake exec() */
            if (!cmd) {
                w += snprintf(output + w, max_len - w,
                    "    let %s: Fact = receive(\"output of %s\") @verified\n",
                    tname, tname);
                continue;
            }

            w += snprintf(output + w, max_len - w,
                "    let %s: str = exec(\"%.120s\")\n",
                tname, cmd);
        }
        w += snprintf(output + w, max_len - w, "\n");
    }

    /* Coordination */
    if (analysis->agent_count >= 2) {
        w += snprintf(output + w, max_len - w,
            "    // Orchestration (was: %s)\n"
            "    coordinate {",
            analysis->orchestration[0] ? analysis->orchestration : "sequential");

        for (int i = 0; i < analysis->agent_count && i < 8; i++) {
            if (i > 0) w += snprintf(output + w, max_len - w, ", ");
            w += snprintf(output + w, max_len - w, "%s",
                analysis->agents[i].name[0] ? analysis->agents[i].name : "agent");
        }

        w += snprintf(output + w, max_len - w,
            "} on(\"execute workflow\") consensus(ProofWeight)\n\n");
    }

    /* Metadata */
    w += snprintf(output + w, max_len - w,
        "    let _source: str = \"%s\" @sovereign\n",
        analysis->repo_name);

    w += snprintf(output + w, max_len - w, "}\n");

    return w;
}

/* ============================================
 * Full Pipeline
 * ============================================ */

int tardy_tf_terraform(const char *repo_path,
                        char *output, int max_len)
{
    tardy_tf_analysis_t analysis;
    int ret = tardy_tf_analyze(repo_path, &analysis);
    if (ret != 0) return ret;

    return tardy_tf_generate(&analysis, output, max_len);
}
