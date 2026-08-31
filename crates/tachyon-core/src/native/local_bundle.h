/* Compiler-owned routes shared by the Linux and Windows WebView hosts. */
#define TACHYON_PATH_LIMIT 4096
#define TACHYON_MESSAGE_LIMIT 65536
#define TACHYON_LOCAL_ORIGIN "__LOCAL_ORIGIN__"
typedef struct { const char *route; const char *document; int language; } TachyonLocalRoute;
static const TachyonLocalRoute TACHYON_LOCAL_ROUTES[] = {
__LOCAL_ROUTES__
  { NULL, NULL, 0 }
};

static int tachyon_hex(char ch) {
  if (ch >= '0' && ch <= '9') return ch - '0';
  if (ch >= 'a' && ch <= 'f') return ch - 'a' + 10;
  if (ch >= 'A' && ch <= 'F') return ch - 'A' + 10;
  return -1;
}

/* Exact origin boundary, bounded percent decoding, no encoded separators or
   dot segments. A hostile URI can never become an ambient filesystem path. */
static int tachyon_local_path(const char *uri, char *path, size_t capacity) {
  size_t origin = strlen(TACHYON_LOCAL_ORIGIN), written = 0;
  if (uri == NULL || strncmp(uri, TACHYON_LOCAL_ORIGIN, origin) != 0 || uri[origin] != '/') return 0;
  for (size_t index = origin; uri[index] != '\0' && uri[index] != '?' && uri[index] != '#'; index++) {
    unsigned char ch = (unsigned char)uri[index];
    if (ch == '%') {
      int high = uri[index + 1] == '\0' ? -1 : tachyon_hex(uri[index + 1]);
      int low = high < 0 || uri[index + 2] == '\0' ? -1 : tachyon_hex(uri[index + 2]);
      if (high < 0 || low < 0) return 0;
      ch = (unsigned char)(high * 16 + low);
      if (ch == '/' || ch == '\\') return 0;
      index += 2;
    }
    if (ch < 32 || ch == 127 || ch == '\\' || ch == ':' || written + 1 >= capacity) return 0;
    path[written++] = (char)ch;
  }
  path[written] = '\0';
  const char *segment = path + 1;
  for (const char *cursor = segment;; cursor++) {
    if (*cursor != '/' && *cursor != '\0') continue;
    size_t length = (size_t)(cursor - segment);
    if ((length == 1 && segment[0] == '.') || (length == 2 && segment[0] == '.' && segment[1] == '.')) return 0;
    if (length > 0 && (segment[length - 1] == '.' || segment[length - 1] == ' ')) return 0;
    if (*cursor == '\0') break;
    if (length == 0) return 0;
    segment = cursor + 1;
  }
  return 1;
}

static const char *tachyon_route_suffix(const char *pattern, const char *path) {
  const char *left = pattern + 1, *right = path + 1;
  if (*left == '\0') return right;
  while (*left != '\0') {
    const char *left_end = strchr(left, '/'), *right_end = strchr(right, '/');
    if (left_end == NULL) left_end = left + strlen(left);
    if (right_end == NULL) right_end = right + strlen(right);
    size_t length = (size_t)(left_end - left), actual = (size_t)(right_end - right);
    if (actual == 0 || (left[0] != '_' && (length != actual || strncmp(left, right, length) != 0))) return NULL;
    left = *left_end == '/' ? left_end + 1 : left_end;
    right = *right_end == '/' ? right_end + 1 : right_end;
  }
  return right;
}

static const TachyonLocalRoute *tachyon_document_route(const char *path) {
  for (size_t index = 0; TACHYON_LOCAL_ROUTES[index].route != NULL; index++) {
    const TachyonLocalRoute *route = &TACHYON_LOCAL_ROUTES[index];
    const char *suffix = tachyon_route_suffix(route->route, path);
    if ((suffix != NULL && *suffix == '\0') || strcmp(path + 1, route->document) == 0) return route;
  }
  return NULL;
}

static int tachyon_bundle_path(const char *path, char *relative, size_t capacity) {
  const TachyonLocalRoute *document = tachyon_document_route(path);
  if (document != NULL) {
    return snprintf(relative, capacity, "%s", document->document) >= 0 && strlen(document->document) < capacity;
  }
  for (size_t index = 0; TACHYON_LOCAL_ROUTES[index].route != NULL; index++) {
    const TachyonLocalRoute *route = &TACHYON_LOCAL_ROUTES[index];
    const char *suffix = tachyon_route_suffix(route->route, path);
    if (suffix == NULL) continue;
    const char *slash = strrchr(route->document, '/');
    size_t prefix = slash == NULL ? 0 : (size_t)(slash - route->document + 1);
    if (prefix + strlen(suffix) >= capacity) return 0;
    memcpy(relative, route->document, prefix);
    memcpy(relative + prefix, suffix, strlen(suffix) + 1);
    return 1;
  }
  if (strlen(path + 1) >= capacity) return 0;
  memcpy(relative, path + 1, strlen(path + 1) + 1);
  return 1;
}

/* Require one literal root route key. Escaped keys fail closed so a second
   spelling cannot override the checked route when the companion parses JSON. */
static int tachyon_payload_string_matches(const char *json, const char *key, const char *expected) {
  int depth = 0, found = 0;
  for (size_t index = 0; json[index] != '\0' && index <= TACHYON_MESSAGE_LIMIT; index++) {
    char ch = json[index];
    if (ch == '{' || ch == '[') { if (++depth > 64) return 0; }
    else if (ch == '}' || ch == ']') { if (--depth < 0) return 0; }
    else if (ch == '"') {
      size_t start = ++index;
      int escaped = 0;
      for (; json[index] != '\0'; index++) {
        if (json[index] == '\\') { escaped = 1; if (json[++index] == '\0') return 0; }
        else if (json[index] == '"') break;
      }
      if (json[index] == '\0') return 0;
      size_t end = index, next = index + 1;
      while (json[next] == ' ' || json[next] == '\n' || json[next] == '\r' || json[next] == '\t') next++;
      if (depth != 1 || json[next] != ':') continue;
      if (escaped) return 0;
      size_t key_length = strlen(key);
      if (end - start != key_length || memcmp(json + start, key, key_length) != 0) continue;
      if (found++) return 0;
      next++;
      while (json[next] == ' ' || json[next] == '\n' || json[next] == '\r' || json[next] == '\t') next++;
      if (json[next++] != '"') return 0;
      size_t length = strlen(expected);
      if (strncmp(json + next, expected, length) != 0 || json[next + length] != '"') return 0;
    }
  }
  return depth == 0 && found == 1;
}

static int tachyon_payload_route_matches(const char *json, const char *route) {
  return tachyon_payload_string_matches(json, "route", route);
}
