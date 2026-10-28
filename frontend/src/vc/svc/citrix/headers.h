typedef unsigned short IU16;
typedef IU16 USHORT;


typedef void *PVOID;
typedef void *LPVOID;

typedef unsigned char IU8;
typedef IU8 UCHAR;
typedef UCHAR *PUCHAR;

typedef signed char I8;
typedef I8 CHAR;

typedef IU8 BYTE;
typedef BYTE *LPBYTE;

#if defined(_WIN32)
typedef unsigned int IU32;
#else
typedef unsigned int IU32;
#endif

typedef IU32 ULONG;

#define TYPEDEF_NATURAL int

typedef unsigned TYPEDEF_NATURAL UINT;

#define TYPEDEF_16BITS short

typedef unsigned TYPEDEF_16BITS UINT16;
typedef unsigned TYPEDEF_16BITS *PUINT16;

#define TYPEDEF_32BITS int

typedef unsigned TYPEDEF_32BITS UINT32;

typedef UINT32 DWORD;




typedef PVOID HND;






const int DLL_MODULE_NAME_MAX_SIZE = 14;

typedef int (PDLLPROCEDURE)(PVOID, PVOID, PUINT16);

typedef struct _DLLLINK {
  USHORT unused1;
  USHORT DllSize;
  USHORT ProcCount;
  PVOID pProcedures;
  PVOID pData;
  PUCHAR unused2;
  BYTE ModuleName[DLL_MODULE_NAME_MAX_SIZE];
  LPVOID pLibMgrCallTable;
  USHORT ModuleDate;
  USHORT ModuleTime;
  ULONG ModuleSize;
  struct _DLLLINK * pNext;
  ULONG DllFlags;
  const char *DllLoad;
  HND LibraryHandle;
} DLLLINK, *PDLLLINK;

typedef struct _VD {
  ULONG ChannelMask;
  PDLLLINK pWdLink;
  int LastError;
  PVOID pPrivate;
} VD, *PVD;

typedef UINT32 (*PLIBPROCEDURE)(void);

typedef struct _VDOPEN {
  PDLLLINK pVdLink;
  PDLLLINK pWdLink;
  ULONG ChannelMask;
  PLIBPROCEDURE reserved2;
  PLIBPROCEDURE pfnStatusMsgProc;
  HND hICAEng;
} VDOPEN, *PVDOPEN;

typedef struct _DLLCLOSE {
  int NotUsed;
} DLLCLOSE, *PDLLCLOSE;

typedef struct _DLLINFO {
  LPBYTE pBuffer;
  USHORT ByteCount;
} DLLINFO, *PDLLINFO;

typedef struct _DLLPOLL {
  ULONG CurrentTime;
} DLLPOLL, * PDLLPOLL;




typedef enum _VDINFOCLASS {
#ifndef unix
    VdLastError,
#endif /* unix */
    VdKillFocus,
    VdSetFocus,
#ifndef unix
    VdMousePosition,
#endif /* unix */
    VdThinWireCache,
    VdWinCEClipboardCheck,
    VdDisableModule,
    VdFlush,
    VdInitWindow,
    VdDestroyWindow,
#ifndef unix
    VdPaint,
#endif /* unix */
    VdThinwireStack,
    VdRealizePaletteFG,
    VdRealizePaletteBG,
    VdInactivate,
#ifndef unix
    VdGetSecurityAccess,
    VdSetSecurityAccess,
    VdGetAudioSecurityAccess,
    VdSetAudioSecurityAccess,
#endif /* unix */
    VdGetPDASecurityAccess,
    VdSetPDASecurityAccess,
#ifndef unix
    VdGetTWNSecurityAccess,
    VdSetTWNSecurityAccess,
#endif /* unix */
    VdSendLogoff,
    VdCCShutdown,

    VdSeamlessHostCommand,
    VdSeamlessQueryInformation,

    VdWindowSwitch,
    VdSetCursor,
    VdSetCursorPos,

    VdEnableState,

    VdIcaControlCommand,

#ifndef unix
    VdVirtualChannel,
    VdWorkArea,
#endif /* unix */
    VdSupportHighThroughput,
#ifndef unix
    VdRenderingMode,
#endif /* unix */
    VdPauseResume,

    VdRedrawNotify,
    VdDisplayCaps,
    VdICOSeamlessFunctions,
    VdPnP,

    VdEuemStartupTimes,

    VdEuemTwCallback,

    VdResizeHotBitmapCache,
    VdSetMonitorLayout,
    VdGUSBGainFocus,
    VdGUSBLoseFocus,

    VdRtpConnectionEstablished,
    VdRtpFinalHandshakeReady,
    VdDimRequest,
    VdGBufferValidateConnection,
    VdCTXIMESendCommand,
    VdMTCommand,
    VdTransportDisconnect,
    VdTransportReconnect,
    VdTransportSwitch,
    VdCamMetrics,
    VdCamStatus,
    VdNoiseSuppressionLevel,
    VdEuemQueryLastRoundtripMs,
    VdCsiMetrics
} VDINFOCLASS;





typedef struct _VDQUERYINFORMATION {
  VDINFOCLASS VdInformationClass;
  LPVOID pVdInformation;
  int VdInformationLength;
  int VdReturnLength;
} VDQUERYINFORMATION, *PVDQUERYINFORMATION;


typedef struct _VDSETINFORMATION {
  VDINFOCLASS VdInformationClass;
  LPVOID pVdInformation;
  int VdInformationLength;
} VDSETINFORMATION, * PVDSETINFORMATION;





const USHORT VDxCOUNT = 8;





const USHORT FLUSH_IMMEDIATELY = 0;

typedef struct _MEMORY_SECTION {
  UINT length;
  LPBYTE pSection;
} MEMORY_SECTION, *LPMEMORY_SECTION;

typedef enum _ICA_TRANSPORT_RELIABILITY_ {
  IcaTransportReliable = 0,
  IcaTransportUnreliable,
  IcaTransportReliableBasicFec,
  IcaTransportCount
} ICA_TRANSPORT_RELIABILITY, *PICA_TRANSPORT_RELIABILITY;

typedef int (*PQUEUEVIRTUALWRITEPROC) (LPVOID, USHORT, LPMEMORY_SECTION, USHORT, USHORT);
typedef int (*PQUEUEVIRTUALWRITEPROCQOS) (LPVOID, USHORT, LPMEMORY_SECTION, USHORT, USHORT, ICA_TRANSPORT_RELIABILITY eReliability, UINT32 *Ticket);

typedef int (*PVDWRITEPROCEDURE)(LPVOID, USHORT, LPBYTE, USHORT);
typedef int (*PVDWRITEPROCEDUREQOS)(LPVOID, USHORT, LPBYTE, USHORT, DWORD, PVOID);
typedef void (*PCBNOTIFTRANSUPDATETOVDCAM)(void);

typedef struct _OUTBUF {
  struct _OUTBUF * pLink;
  LPBYTE pMemory;
  LPBYTE pBuffer;
  USHORT MaxByteCount;
  USHORT ByteCount;
  ICA_TRANSPORT_RELIABILITY eReliability;
  UINT32 *pTicket;
} OUTBUF, *POUTBUF;

typedef enum _SETINFOCLASS {
  CallbackComplete
} SETINFOCLASS, *PSETINFOCLASS;

typedef enum _QUERYINFOCLASS {
  QueryHostVersion,
  OpenVirtualChannel
} QUERYINFOCLASS, *PQUERYINFOCLASS;

typedef int (*POUTBUFALLOCPROC)(LPVOID, POUTBUF *);
typedef void (*POUTBUFFREEPROC)(LPVOID, POUTBUF);
typedef int (*PPROCESSINPUTPROC)(LPVOID, LPBYTE, USHORT, int);
typedef int (*PSETINFOPROC)(LPVOID, SETINFOCLASS, LPBYTE, USHORT);
typedef void (*PIOHOOKPROC)(LPBYTE, USHORT);

typedef int (*PQUERYINFOPROC)(LPVOID, QUERYINFOCLASS, LPBYTE, USHORT);
typedef int (*POUTBUFRESERVEPROC)(LPVOID, USHORT);
typedef int (*POUTBUFAPPENDPROC)(LPVOID, LPBYTE, USHORT);
typedef int (*POUTBUFWRITEPROC)(LPVOID);
typedef int (*PAPPENDVDHEADERPROC)(LPVOID, USHORT, USHORT);

typedef struct _VDWRITEHOOK {
  USHORT Type;
  LPVOID pVdData;
  union {
    PVDWRITEPROCEDURE pProc;
    PVDWRITEPROCEDUREQOS pProcQos;
  };
  LPVOID pWdData;
  POUTBUFRESERVEPROC pOutBufReserveProc;
  POUTBUFAPPENDPROC pOutBufAppendProc;
  POUTBUFWRITEPROC pOutBufWriteProc;
  PAPPENDVDHEADERPROC pAppendVdHeaderProc;
  USHORT MaximumWriteSize;
  union {
    PQUEUEVIRTUALWRITEPROC pQueueVirtualWriteProc;
    PQUEUEVIRTUALWRITEPROCQOS pQueueVirtualWriteProcQos;
  };
} VDWRITEHOOK, *PVDWRITEHOOK;

typedef struct _WD * PWD;

const unsigned WDxQUERYINFORMATION = 6;
const unsigned WDxSETINFORMATION = 7;
const unsigned WDxCOUNT = 8;

typedef struct _OPENVIRTUALCHANNEL {
  LPVOID  pVCName;
  USHORT  Channel;
} OPENVIRTUALCHANNEL, *POPENVIRTUALCHANNEL;

typedef enum _WDINFOCLASS {
  WdClientData,
  WdStatistics,
  WdLastError,
  WdConnect,
  WdDisconnect,
  WdKillFocus,
  WdSetFocus,
  WdEnablePassThrough,
  WdDisablePassThrough,
  WdVdAddress,
  WdVirtualWriteHook,
  WdAddReadHook,
  WdRemoveReadHook,
  WdAddWriteHook,
  WdRemoveWriteHook,
  WdModemStatus,
  WdXferBufferSize,
  WdCharCode,
  WdScanCode,
  WdMouseInfo,
  WdInitWindow,
  WdDestroyWindow,
  WdRedraw,
  WdThinwireStack,
  WdHostVersion,
  WdRealizePaletteFG,
  WdRealizePaletteBG,
  WdInactivate,
  WdSetProductID,
  WdGetTerminateInfo,
  WdRaiseSoftkey,
  WdLowerSoftkey,
  WdIOStatus,
  WdOpenVirtualChannel,
  WdCache,
  WdGetInfoData,
  WdWindowSwitch,
#if defined(UNICODESUPPORT) || defined(USE_EUKS)
  WdUnicodeCode,
#endif
#ifdef PACKET_KEYSYM_SUPPORT
  WdKeysymCode,
#endif
#if defined(_WIN32)
  WdSetNetworkEvent,
#endif
  WdPassThruLogoff,
  WdClientIdInfo,
  WdPartialDisconnect,
  WdDesktopInfo,
  WdSeamlessHostCommand,
  WdSeamlessQueryInformation,
#if defined(UNICODESUPPORT) || defined(USE_EUKS)
  WdZlRegisterUnicodeHook,
  WdZLUnRegisterUnicodeHook,
#endif
  WdZLRegisterScanCodeHook,
  WdZlUnregisterScanCodeHook,
  WdIcmQueryStatus,
  WdIcmSendPingRequest,
  WdSetCursor,
  WdFullScreenMode,
  WdFullScreenPaint,
  WdSeamlessInfo,
  WdCodePage,
  WdIcaControlCommand,
  WdReconnectInfo,
  WdServerSupportBWControl4Printing,
  WdVirtualChannel,
  WdGetLatencyInformation,
  WdKeyboardInput,
  WdMouseInput,
  WdCredentialPassing,
  WdRenderingMode,
  WdPauseResume,
  WdQueryMMWindowInfo,
  WdGetICAWindowInfo,
  WdICOSeamlessFunctions,
#ifdef USE_EUKS
  WdEUKSVersion,
#endif
  WdSetC2HPriority,
  WdPnP,
  WdEuemEndSLCD,
  WdEuemStartupTimes,
  WdEuemTwCallback,
  WdSessionIsReconnected,
  WdUserActivity,
#ifdef WINCE
  WdEuemApplicationName,
#endif
  WdLicensedFeatures,
  WdResizeHotBitmapCache,
  WdLockDisplay,
  WdRtpSetupInformation,
  WdRtpInitClientHandshake,
  WdRtpSetup,
  WdQueryVCNumbersForVD,
  WDCheckOutTicket,
  WDCheckInTicket,
  WdMarshallVdInfo,
  WdVirtualWriteHookQos,
  WdQueryEdt,
  WdQueryMaxUnreliablePayload,
  WdSubscribeDesktopInfoChange,
  WdUnsubscribeDesktopInfoChange,
  WdSendMTCommand,
  WdUpdateMonitorLayout,
  WdSubscribeMonitorLayoutChange,
  WdUnsubscribeMonitorLayoutChange,
} WDINFOCLASS;

#define MAX_ERRORMESSAGE 288

typedef struct _VDLASTERROR {
  int Error;
  char Message[MAX_ERRORMESSAGE];
} VDLASTERROR, * PVDLASTERROR;

typedef struct _WDQUERYINFORMATION {
  WDINFOCLASS WdInformationClass;
  LPVOID pWdInformation;
  USHORT WdInformationLength;
  USHORT WdReturnLength;
} WDQUERYINFORMATION, *PWDQUERYINFORMATION;

typedef struct _WDSETINFORMATION {
  WDINFOCLASS WdInformationClass;
  LPVOID pWdInformation;
  USHORT WdInformationLength;
} WDSETINFORMATION, * PWDSETINFORMATION;





#pragma pack(1)




typedef enum _MODULECLASS {
  Module_UserInterface,
  Module_UserInterfaceExt,
  Module_WinstationDriver,
  Module_VirtualDriver,
  Module_ProtocolDriver,
  Module_TransportDriver,
  Module_NameResolver,
  Module_NameEnumerator,
  Module_Scripting,
  Module_SubDriver,
  ModuleClass_Maximum
} MODULECLASS;

typedef struct _MODULE_C2H {
  USHORT ByteCount;
  BYTE ModuleCount;
  BYTE ModuleClass;
  BYTE VersionL;
  BYTE VersionH;
  CHAR ModuleName[13];
  CHAR HostModuleName[9];
  USHORT ModuleDate;
  USHORT ModuleTime;
  ULONG ModuleSize;
} MODULE_C2H, *PMODULE_C2H;

typedef enum _VIRTUALFLOWCLASS {
  VirtualFlow_None,
  VirtualFlow_Ack,
  VirtualFlow_Delay,
  VirtualFlow_Cdm
} VIRTUALFLOWCLASS;

typedef struct _VDFLOWACK {
  USHORT MaxWindowSize;
  USHORT WindowSize;
} VDFLOWACK, *PVDFLOWACK;

typedef struct _VDFLOWDELAY {
  ULONG DelayTime;
} VDFLOWDELAY, *PVDFLOWDELAY;

typedef struct _VDFLOWCDM {
  USHORT MaxWindowSize;
  USHORT MaxByteCount;
} VDFLOWCDM, *PVDFLOWCDM;

typedef struct _VDFLOW {
  BYTE BandwidthQuota;
  BYTE Flow;
  BYTE Pad1[2];
  union _VDFLOWU {
    VDFLOWACK Ack;
    VDFLOWDELAY Delay;
    VDFLOWCDM Cdm;
  } VDFLOWU ;
} VDFLOW, *PVDFLOW;

typedef struct _VD_C2H {
  MODULE_C2H Header;
  ULONG ChannelMask;
  VDFLOW Flow;
} VD_C2H, *PVD_C2H;

typedef struct {
  VD_C2H Header;
} SOXY_C2H, *PSOXY_C2H;



#pragma pack()






const int CLIENT_STATUS_SUCCESS = 0;
const int CLIENT_STATUS_ERROR_RETRY = 30;

const int CLIENT_ERROR = 1000;
const int CLIENT_ERROR_BUFFER_TOO_SMALL = 1004;
const int CLIENT_ERROR_NULL_MEM_POINTER = 1011;
const int CLIENT_ERROR_NO_OUTBUF = 1016;
