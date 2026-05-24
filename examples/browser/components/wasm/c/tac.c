static unsigned char INPUT[2048];
static unsigned char OUTPUT[128];
static unsigned int OUTPUT_LEN = 0;
static unsigned int CLICKS = 0;

static const char PREFIX[] = "{\"state\":{\"clicks\":";
static const char SUFFIX[] = ",\"label\":\"C\"}}";

static unsigned int copy_bytes(unsigned int pos, const char *source, unsigned int len) {
  for (unsigned int i = 0; i < len; i++) {
    OUTPUT[pos++] = (unsigned char)source[i];
  }
  return pos;
}

static unsigned int write_uint(unsigned int value, unsigned int pos) {
  unsigned char digits[10];
  unsigned int cursor = 10;
  do {
    digits[--cursor] = (unsigned char)('0' + (value % 10));
    value /= 10;
  } while (value > 0);
  while (cursor < 10) {
    OUTPUT[pos++] = digits[cursor++];
  }
  return pos;
}

static void write_state(void) {
  unsigned int pos = 0;
  pos = copy_bytes(pos, PREFIX, sizeof(PREFIX) - 1);
  pos = write_uint(CLICKS, pos);
  pos = copy_bytes(pos, SUFFIX, sizeof(SUFFIX) - 1);
  OUTPUT_LEN = pos;
}

static unsigned int read_clicks(unsigned char *payload, unsigned int len) {
  const char marker[] = "\"clicks\":";
  const unsigned int marker_len = sizeof(marker) - 1;
  for (unsigned int i = 0; i + marker_len <= len; i++) {
    unsigned int matched = 1;
    for (unsigned int j = 0; j < marker_len; j++) {
      if (payload[i + j] != (unsigned char)marker[j]) {
        matched = 0;
        break;
      }
    }
    if (matched) {
      unsigned int value = 0;
      unsigned int cursor = i + marker_len;
      while (cursor < len && payload[cursor] >= '0' && payload[cursor] <= '9') {
        value = value * 10 + (unsigned int)(payload[cursor] - '0');
        cursor++;
      }
      return value;
    }
  }
  return 0;
}

__attribute__((export_name("alloc")))
unsigned char *tachyon_alloc(unsigned int size) {
  return size > sizeof(INPUT) ? 0 : INPUT;
}

__attribute__((export_name("dealloc")))
void tachyon_dealloc(unsigned char *ptr, unsigned int len) {}

__attribute__((export_name("init")))
void tachyon_init(unsigned char *ptr, unsigned int len) {
  CLICKS = 0;
  write_state();
}

__attribute__((export_name("call")))
void tachyon_call(unsigned char *method_ptr, unsigned int method_len, unsigned char *payload_ptr, unsigned int payload_len) {
  CLICKS = read_clicks(payload_ptr, payload_len) + 1;
  write_state();
}

__attribute__((export_name("output_ptr")))
unsigned int tachyon_output_ptr(void) {
  return (unsigned int)OUTPUT;
}

__attribute__((export_name("output_len")))
unsigned int tachyon_output_len(void) {
  return OUTPUT_LEN;
}
