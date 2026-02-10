// Minimal native runtime for host intrinsics used by the bootstrap path.
//
// This file is compiled and linked into native binaries produced by `kooixc native`.
// It intentionally depends only on libc.
//
// Text is represented as a NUL-terminated `char*` (same as `i8*` in LLVM).

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(__unix__) || defined(__APPLE__)
#include <sys/resource.h>
#endif

typedef struct KxEnum {
  uint8_t tag;
  uint64_t payload;
} KxEnum;

typedef struct KxStrNode {
  char* value;
  struct KxStrNode* next;
} KxStrNode;

// Best-effort: increase stack limit for deeply recursive Stage1 tooling when running as a native
// executable. No-op if unsupported or if raising the limit fails.
void kx_runtime_init(void) {
#if defined(__unix__) || defined(__APPLE__)
  struct rlimit lim;
  if (getrlimit(RLIMIT_STACK, &lim) != 0) {
    return;
  }

  const rlim_t target = (rlim_t)(64ULL * 1024ULL * 1024ULL);
  if (lim.rlim_cur >= target) {
    return;
  }

  rlim_t new_cur = target;
  if (lim.rlim_max != RLIM_INFINITY && new_cur > lim.rlim_max) {
    new_cur = lim.rlim_max;
  }
  if (new_cur <= lim.rlim_cur) {
    return;
  }

  lim.rlim_cur = new_cur;
  (void)setrlimit(RLIMIT_STACK, &lim);
#endif
}

static char* kx_strdup(const char* s) {
  if (!s) {
    return NULL;
  }
  size_t n = strlen(s);
  char* out = (char*)malloc(n + 1);
  if (!out) {
    return NULL;
  }
  memcpy(out, s, n);
  out[n] = '\0';
  return out;
}

static char* kx_strcat2(const char* a, const char* b) {
  size_t al = a ? strlen(a) : 0;
  size_t bl = b ? strlen(b) : 0;
  char* out = (char*)malloc(al + bl + 1);
  if (!out) {
    return NULL;
  }
  if (a && al) {
    memcpy(out, a, al);
  }
  if (b && bl) {
    memcpy(out + al, b, bl);
  }
  out[al + bl] = '\0';
  return out;
}

static char* kx_strcat3(const char* a, const char* b, const char* c) {
  char* ab = kx_strcat2(a, b);
  if (!ab) {
    return NULL;
  }
  char* out = kx_strcat2(ab, c);
  return out;
}

static int kx_has_extension(const char* path) {
  if (!path) {
    return 0;
  }
  const char* slash = strrchr(path, '/');
  const char* dot = strrchr(path, '.');
  if (!dot) {
    return 0;
  }
  if (slash && dot < slash) {
    return 0;
  }
  return 1;
}

static char* kx_add_extension(const char* path, const char* ext) {
  if (!path) {
    return NULL;
  }
  if (kx_has_extension(path)) {
    return kx_strdup(path);
  }
  return kx_strcat2(path, ext);
}

static char* kx_prefix_up(const char* path, int up_levels) {
  const char* prefix = "../";
  size_t pl = strlen(prefix);
  size_t bl = path ? strlen(path) : 0;
  size_t n = (size_t)up_levels;
  char* out = (char*)malloc(n * pl + bl + 1);
  if (!out) {
    return NULL;
  }
  size_t off = 0;
  for (size_t i = 0; i < n; i++) {
    memcpy(out + off, prefix, pl);
    off += pl;
  }
  if (path && bl) {
    memcpy(out + off, path, bl);
    off += bl;
  }
  out[off] = '\0';
  return out;
}

static char* kx_dirname_with_slash(const char* path) {
  if (!path) {
    return kx_strdup("");
  }
  const char* slash = strrchr(path, '/');
  if (!slash) {
    return kx_strdup("");
  }
  size_t n = (size_t)(slash - path) + 1; // include trailing '/'
  char* out = (char*)malloc(n + 1);
  if (!out) {
    return NULL;
  }
  memcpy(out, path, n);
  out[n] = '\0';
  return out;
}

static char* kx_resolve_import_path(const char* base_dir, const char* raw) {
  if (!raw) {
    return NULL;
  }
  char* joined = NULL;
  if (raw[0] == '/') {
    joined = kx_strdup(raw);
  } else {
    joined = kx_strcat2(base_dir ? base_dir : "", raw);
  }
  if (!joined) {
    return NULL;
  }
  char* with_ext = kx_add_extension(joined, ".kooix");
  return with_ext;
}

static char* kx_read_file_exact(const char* path, char** err_out) {
  if (err_out) {
    *err_out = NULL;
  }
  FILE* f = fopen(path, "rb");
  if (!f) {
    if (err_out) {
      *err_out = kx_strcat3("failed to read file '", path ? path : "(null)", "'");
    }
    return NULL;
  }
  if (fseek(f, 0, SEEK_END) != 0) {
    fclose(f);
    if (err_out) {
      *err_out = kx_strdup("failed to seek file");
    }
    return NULL;
  }
  long size = ftell(f);
  if (size < 0) {
    fclose(f);
    if (err_out) {
      *err_out = kx_strdup("failed to stat file");
    }
    return NULL;
  }
  if (fseek(f, 0, SEEK_SET) != 0) {
    fclose(f);
    if (err_out) {
      *err_out = kx_strdup("failed to seek file");
    }
    return NULL;
  }
  char* buf = (char*)malloc((size_t)size + 1);
  if (!buf) {
    fclose(f);
    if (err_out) {
      *err_out = kx_strdup("out of memory");
    }
    return NULL;
  }
  size_t read_n = fread(buf, 1, (size_t)size, f);
  fclose(f);
  buf[read_n] = '\0';
  return buf;
}

static char* kx_read_file_with_search(const char* raw, char** err_out) {
  char* err = NULL;
  char* path0 = kx_add_extension(raw, ".kooix");
  if (!path0) {
    if (err_out) {
      *err_out = kx_strdup("out of memory");
    }
    return NULL;
  }

  char* out = kx_read_file_exact(path0, &err);
  if (out) {
    if (err_out) {
      *err_out = NULL;
    }
    return out;
  }

  // Search parent directories (mirrors Stage0 intrinsic behavior for tests).
  for (int up = 1; up <= 8; up++) {
    char* candidate = kx_prefix_up(path0, up);
    if (!candidate) {
      break;
    }
    char* err2 = NULL;
    out = kx_read_file_exact(candidate, &err2);
    if (out) {
      if (err_out) {
        *err_out = NULL;
      }
      return out;
    }
  }

  if (err_out) {
    *err_out = err ? err : kx_strdup("failed to read file");
  }
  return NULL;
}

static int kx_is_ident_start(char c) {
  return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_';
}

static int kx_is_ident_continue(char c) {
  return kx_is_ident_start(c) || (c >= '0' && c <= '9');
}

static void kx_skip_ws_and_line_comments(const char* s, size_t* idx) {
  for (;;) {
    char c = s[*idx];
    if (c == ' ' || c == '\n' || c == '\r' || c == '\t') {
      (*idx)++;
      continue;
    }
    if (c == '/' && s[*idx + 1] == '/') {
      (*idx) += 2;
      while (s[*idx] && s[*idx] != '\n') {
        (*idx)++;
      }
      continue;
    }
    return;
  }
}

static int kx_visited_contains(KxStrNode* visited, const char* path) {
  for (KxStrNode* cur = visited; cur; cur = cur->next) {
    if (cur->value && path && strcmp(cur->value, path) == 0) {
      return 1;
    }
  }
  return 0;
}

static void kx_visited_push(KxStrNode** visited, const char* path) {
  if (!visited || !path) {
    return;
  }
  KxStrNode* node = (KxStrNode*)malloc(sizeof(KxStrNode));
  if (!node) {
    return;
  }
  node->value = kx_strdup(path);
  node->next = *visited;
  *visited = node;
}

static char* kx_file_marker(const char* path) {
  return kx_strcat3("// --- file: ", path ? path : "(null)", " ---\n");
}

static void kx_load_file(const char* path, KxStrNode** visited, char** combined, char** err_out) {
  if (err_out && *err_out) {
    return;
  }
  if (!path || !combined) {
    if (err_out) {
      *err_out = kx_strdup("invalid arguments");
    }
    return;
  }

  if (kx_visited_contains(visited ? *visited : NULL, path)) {
    return;
  }
  if (visited) {
    kx_visited_push(visited, path);
  }

  char* err = NULL;
  char* src = kx_read_file_with_search(path, &err);
  if (!src) {
    if (err_out) {
      *err_out = err ? err : kx_strdup("failed to read file");
    }
    return;
  }

  char* base_dir = kx_dirname_with_slash(path);
  if (!base_dir) {
    if (err_out) {
      *err_out = kx_strdup("out of memory");
    }
    return;
  }

  // Collect include-style imports at depth 0 and load them first.
  size_t i = 0;
  int depth = 0;
  while (src[i]) {
    kx_skip_ws_and_line_comments(src, &i);
    char c = src[i];
    if (!c) {
      break;
    }
    if (c == '{' || c == '(' || c == '[') {
      depth++;
      i++;
      continue;
    }
    if (c == '}' || c == ')' || c == ']') {
      if (depth > 0) {
        depth--;
      }
      i++;
      continue;
    }

    if (depth == 0 && kx_is_ident_start(c)) {
      size_t start = i;
      i++;
      while (kx_is_ident_continue(src[i])) {
        i++;
      }
      size_t len = i - start;
      if (len == 6 && strncmp(src + start, "import", 6) == 0) {
        kx_skip_ws_and_line_comments(src, &i);
        if (src[i] == '"') {
          i++;
          size_t ps = i;
          while (src[i] && src[i] != '"') {
            i++;
          }
          if (src[i] == '"') {
            size_t plen = i - ps;
            char* raw_import = (char*)malloc(plen + 1);
            if (raw_import) {
              memcpy(raw_import, src + ps, plen);
              raw_import[plen] = '\0';
            }
            i++; // closing quote
            kx_skip_ws_and_line_comments(src, &i);
            if (src[i] == ';' && raw_import) {
              i++;
              char* resolved = kx_resolve_import_path(base_dir, raw_import);
              if (resolved) {
                kx_load_file(resolved, visited, combined, err_out);
              }
            }
          }
        }
      }
      continue;
    }

    i++;
  }

  // Append file content (deps first).
  char* marker = kx_file_marker(path);
  char* next = kx_strcat3(*combined ? *combined : "", marker ? marker : "", src);
  if (!next) {
    if (err_out) {
      *err_out = kx_strdup("out of memory");
    }
    return;
  }
  char* next2 = kx_strcat2(next, "\n\n");
  if (!next2) {
    if (err_out) {
      *err_out = kx_strdup("out of memory");
    }
    return;
  }
  *combined = next2;
}

// Signature chosen to map cleanly into the compiler's enum layout:
// - Result<Text, Text> is represented as `{ i8 tag, i64 payload_word }` heap-allocated.
KxEnum* kx_host_load_source_map(const char* entry_path) {
  char* combined = kx_strdup("");
  char* err = NULL;
  KxStrNode* visited = NULL;

  kx_load_file(entry_path, &visited, &combined, &err);

  KxEnum* out = (KxEnum*)malloc(sizeof(KxEnum));
  if (!out) {
    return NULL;
  }

  if (err) {
    out->tag = 1; // Err
    out->payload = (uint64_t)(uintptr_t)err;
  } else {
    out->tag = 0; // Ok
    out->payload = (uint64_t)(uintptr_t)combined;
  }
  return out;
}

void kx_host_eprintln(const char* s) {
  if (!s) {
    fputs("(null)\n", stderr);
    return;
  }
  fputs(s, stderr);
  fputc('\n', stderr);
}

KxEnum* kx_host_write_file(const char* path, const char* content) {
  KxEnum* out = (KxEnum*)malloc(sizeof(KxEnum));
  if (!out) {
    return NULL;
  }

  if (!path) {
    out->tag = 1; // Err
    out->payload = (uint64_t)(uintptr_t)kx_strdup("host_write_file: path is null");
    return out;
  }

  FILE* f = fopen(path, "wb");
  if (!f) {
    out->tag = 1; // Err
    out->payload =
        (uint64_t)(uintptr_t)kx_strcat3("failed to open for write: ", path, "");
    return out;
  }

  size_t n = content ? strlen(content) : 0;
  size_t w = 0;
  if (n) {
    w = fwrite(content, 1, n, f);
  }
  int close_ok = (fclose(f) == 0);

  if (!close_ok || w != n) {
    out->tag = 1; // Err
    out->payload =
        (uint64_t)(uintptr_t)kx_strcat3("failed to write file: ", path, "");
    return out;
  }

  out->tag = 0; // Ok
  out->payload = 0; // Int(0)
  return out;
}

char* kx_text_concat(const char* a, const char* b) {
  return kx_strcat2(a ? a : "", b ? b : "");
}

char* kx_int_to_text(int64_t v) {
  // Portable enough for our bootstrap needs; libc-only.
  int n = snprintf(NULL, 0, "%lld", (long long)v);
  if (n < 0) {
    return NULL;
  }
  char* out = (char*)malloc((size_t)n + 1);
  if (!out) {
    return NULL;
  }
  snprintf(out, (size_t)n + 1, "%lld", (long long)v);
  out[n] = '\0';
  return out;
}
