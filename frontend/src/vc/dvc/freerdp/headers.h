#if defined(_WIN32)
#else
   #ifndef USE_WIN_DWORD_RANGE
      #define USE_WIN_DWORD_RANGE
   #endif
#endif

typedef void VOID;
typedef VOID *PVOID;
typedef VOID *LPVOID;

typedef unsigned long ULONG;
typedef ULONG *PULONG;

typedef unsigned int UINT;
typedef unsigned int UINT32;

typedef int INT;

typedef unsigned char IU8;
typedef IU8 UCHAR;
typedef UCHAR *PUCHAR;

typedef char CHAR;
typedef CHAR *PCHAR;

typedef IU8 BYTE;
typedef BYTE *LPBYTE;

typedef int BOOL;

const BOOL TRUE = 1;
const BOOL FALSE = 0;

#ifndef USE_WIN_DWORD_RANGE
#ifdef __APPLE__
#include <stdint.h>
typedef uint32_t      DWORD;
#else
typedef unsigned long DWORD;
#endif
#else
typedef unsigned int DWORD;
#endif

typedef DWORD *LPDWORD;


#define CHANNEL_RC_OK                           0
#define ERROR_NO_DATA                  0x000000E8

typedef struct s_IWTSVirtualChannelManager IWTSVirtualChannelManager;
typedef struct s_IWTSListener IWTSListener;
typedef struct s_IWTSVirtualChannel IWTSVirtualChannel;

typedef struct s_IWTSPlugin IWTSPlugin;
typedef struct s_IWTSListenerCallback IWTSListenerCallback;
typedef struct s_IWTSVirtualChannelCallback IWTSVirtualChannelCallback;

struct s_IWTSListener {
  UINT (*GetConfiguration)(IWTSListener* pListener,
                           void** ppPropertyBag);
  void* pInterface;
};

struct s_IWTSVirtualChannel {
  UINT (*Write)(IWTSVirtualChannel* pChannel,
                ULONG cbSize,
                const BYTE* pBuffer,
                void* pReserved);
  UINT (*Close)(IWTSVirtualChannel* pChannel);
};

struct s_IWTSVirtualChannelManager {
  UINT (*CreateListener)(IWTSVirtualChannelManager* pChannelMgr,
                         const char* pszChannelName,
                         ULONG ulFlags,
                         IWTSListenerCallback* pListenerCallback,
                         IWTSListener** ppListener);
  UINT32 (*GetChannelId)(IWTSVirtualChannel* channel);
  IWTSVirtualChannel* (*FindChannelById)(IWTSVirtualChannelManager* pChannelMgr,
                                         UINT32 ChannelId);
  const char* (*GetChannelName)(IWTSVirtualChannel* channel);
  UINT (*DestroyListener)(IWTSVirtualChannelManager* pChannelMgr,
                          IWTSListener* ppListener);
};

struct s_IWTSPlugin {
  UINT (*Initialize)(IWTSPlugin* pPlugin,
                     IWTSVirtualChannelManager* pChannelMgr);
  UINT (*Connected)(IWTSPlugin* pPlugin);
  UINT (*Disconnected)(IWTSPlugin* pPlugin,
                       DWORD dwDisconnectCode);
  UINT (*Terminated)(IWTSPlugin* pPlugin);
  UINT (*Attached)(IWTSPlugin* pPlugin);
  UINT (*Detached)(IWTSPlugin* pPlugin);
  void* pInterface;
};

struct s_IWTSListenerCallback {
  UINT (*OnNewChannelConnection)(IWTSListenerCallback* pListenerCallback,
                                 IWTSVirtualChannel* pChannel,
                                 BYTE* Data,
                                 BOOL* pbAccept,
                                 IWTSVirtualChannelCallback** ppCallback);
  void* pInterface;
};

typedef struct s_wStreamPool wStreamPool;

typedef struct {
  BYTE* buffer;
  BYTE* pointer;
  ULONG length;
  ULONG capacity;

  DWORD count;
  wStreamPool* pool;
  BOOL isAllocatedStream;
  BOOL isOwner;
} wStream;

struct s_IWTSVirtualChannelCallback {
  UINT (*OnDataReceived)(IWTSVirtualChannelCallback* pChannelCallback,
                         wStream* data);
  UINT (*OnOpen)(IWTSVirtualChannelCallback* pChannelCallback);
  UINT (*OnClose)(IWTSVirtualChannelCallback* pChannelCallback);
  void* pInterface;
};

typedef struct rdp_context rdpContext;
typedef struct rdp_settings rdpSettings;

typedef struct {
  int argc;
  char** argv;
} ADDIN_ARGV;

typedef struct s_IDRDYNVC_ENTRY_POINTS IDRDYNVC_ENTRY_POINTS;

struct s_IDRDYNVC_ENTRY_POINTS {
  UINT (*RegisterPlugin)(IDRDYNVC_ENTRY_POINTS* pEntryPoints,
                         const char* name,
                         IWTSPlugin* pPlugin);
  IWTSPlugin* (*GetPlugin)(IDRDYNVC_ENTRY_POINTS* pEntryPoints,
                           const char* name);
  const ADDIN_ARGV* (*GetPluginData)(IDRDYNVC_ENTRY_POINTS* pEntryPoints);
  rdpSettings* (*GetRdpSettings)(IDRDYNVC_ENTRY_POINTS* pEntryPoints);
  rdpContext* (*GetRdpContext)(IDRDYNVC_ENTRY_POINTS* pEntryPoints);
};
