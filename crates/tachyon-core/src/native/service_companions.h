typedef struct { const char *service; int language; } TachyonServiceCompanion;
static const TachyonServiceCompanion TACHYON_SERVICE_COMPANIONS[] = {
__SERVICE_COMPANIONS__
  { NULL, 0 }
};

static int tachyon_service_companion(const char *payload) {
  for (size_t index = 0; TACHYON_SERVICE_COMPANIONS[index].service != NULL; index++) {
    const TachyonServiceCompanion *entry = &TACHYON_SERVICE_COMPANIONS[index];
    if (tachyon_payload_string_matches(payload, "service", entry->service)) return entry->language;
  }
  return 0;
}
