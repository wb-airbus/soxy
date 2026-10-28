#include "../x11/headers.h"

#define ALIGN64 __attribute__((aligned(8)))
#define UINT64 unsigned long long
#define UINT16 unsigned short
#define UINT8 unsigned char

struct rdp_context  {
  ALIGN64 void* instance;
  ALIGN64 void* peer;
  ALIGN64 BOOL ServerMode;
  ALIGN64 UINT32 LastError;
  UINT64 paddingA[16 - 4];
  ALIGN64 int argc;
  ALIGN64 char** argv;
  ALIGN64 void* pubSub;
  ALIGN64 void* channelErrorEvent;
  ALIGN64 UINT channelErrorNum;
  ALIGN64 char* errorDescription;

  UINT64 paddingB[32 - 22];
  ALIGN64 void* rdp;
  ALIGN64 void* gdi;
  ALIGN64 void* rail;
  ALIGN64 void* cache;
  ALIGN64 void* channels;
  ALIGN64 void* graphics;
  ALIGN64 void* input;
  ALIGN64 void* update;
  ALIGN64 void* settings;
  ALIGN64 void* metrics;
  ALIGN64 void* codecs;
  ALIGN64 void* autodetect;
  UINT64 paddingC1[45 - 44];
  ALIGN64 int disconnectUltimatum;
  UINT64 paddingC[64 - 46];

  ALIGN64 void* dump;
  ALIGN64 void* log;

  UINT64 paddingD[96 - 66];
  UINT64 paddingE[128 - 96];
};

typedef struct {
  UINT32 cbSize;
  UINT32 protocolVersion;
  void *pVirtualChannelInit;
  void *pVirtualChannelOpen;
  void *pVirtualChannelClose;
  void *pVirtualChannelWrite;

  UINT32 MagicNumber;
  void* pExtendedData;
  void* pInterface;
  struct rdp_context* rdpContext;
} CHANNEL_ENTRY_POINTS_FREERDP;

typedef CHANNEL_ENTRY_POINTS_FREERDP *PCHANNEL_ENTRY_POINTS_FREERDP;

const UINT32 FREERDP_CHANNEL_MAGIC_NUMBER = 0x46524450;


typedef struct s_IDRDYNVC_ENTRY_POINTS IDRDYNVC_ENTRY_POINTS;

struct s_IDRDYNVC_ENTRY_POINTS {
  UINT (*RegisterPlugin)(IDRDYNVC_ENTRY_POINTS* pEntryPoints,
                         const char* name,
                         void* pPlugin);
  void* (*GetPlugin)(IDRDYNVC_ENTRY_POINTS* pEntryPoints,
                     const char* name);
  const void* (*GetPluginData)(IDRDYNVC_ENTRY_POINTS* pEntryPoints);
  void* (*GetRdpSettings)(IDRDYNVC_ENTRY_POINTS* pEntryPoints);
  struct rdp_context* (*GetRdpContext)(IDRDYNVC_ENTRY_POINTS* pEntryPoints);
};


typedef BOOL (*freerdp_input_send_keyboard_event_ex)(void* rdp_input,
                                                     BOOL down,
                                                     BOOL repeat,
                                                     UINT32 rdp_scancode);

typedef DWORD (*freerdp_keyboard_init)(DWORD keyboardLayoutId);

typedef DWORD (*freerdp_keyboard_get_rdp_scancode_from_x11_keycode)(DWORD keycode);
