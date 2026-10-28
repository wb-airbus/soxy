#ifndef X11_HEADERS
#define X11_HEADERS

#include "../headers.h"

typedef void Display;
typedef void *Window;
typedef int Bool;
typedef unsigned long Time;

/*
 * X11
 */

typedef Display *(*XOpenDisplay)(char *);

typedef unsigned long KeySym;
typedef unsigned char KeyCode;
typedef KeyCode (*XKeysymToKeycode)(Display *display, KeySym keysym);

typedef unsigned int (*XkbKeysymToModifiers)(Display *display, KeySym ks);

typedef struct {
  unsigned char mask;
  unsigned char real_mods;
  unsigned short vmods;
} XkbModsRec, *XkbModsPtr;

const unsigned char ShiftMask = 0x01;
const unsigned char LockMask = 0x02;
const unsigned char ControlMask = 0x04;
const unsigned char Mod1Mask = 0x08;
const unsigned char Mod2Mask = 0x10;
const unsigned char Mod3Mask = 0x20;
const unsigned char Mod4Mask = 0x40;
const unsigned char Mod5Mask = 0x80;

typedef struct {
  BOOL active;
  unsigned char level;
  XkbModsRec mods;
} XkbKTMapEntryRec, *XkbKTMapEntryPtr;

typedef unsigned long Atom;

typedef struct {
 XkbModsRec mods;
 unsigned char num_levels;
 unsigned char map_count;
 XkbKTMapEntryPtr map;
 XkbModsPtr preserve;
 Atom name;
 Atom *level_names;
} XkbKeyTypeRec, *XkbKeyTypePtr;

const unsigned int XkbNumKbdGroups = 4;

typedef struct {
  unsigned char kt_index[XkbNumKbdGroups];
  unsigned char group_info;
  unsigned char width;
  unsigned short offset;
} XkbSymMapRec, *XkbSymMapPtr;

typedef struct {
  unsigned char size_types;
  unsigned char num_types;
  XkbKeyTypePtr types;
  unsigned short size_syms;
  unsigned short num_syms;
  KeySym *syms;
  XkbSymMapPtr key_sym_map;
  unsigned char *modmap;
} XkbClientMapRec, *XkbClientMapPtr;

typedef struct {
  Display *display;
  unsigned short flags;
  unsigned short device_spec;
  KeyCode min_key_code;
  KeyCode max_key_code;
  void *ctrls;
  void *server;
  XkbClientMapPtr map;
  void *indicators;
  void *names;
  void *compat;
  void *geom;
} XkbDescRec, *XkbDescPtr;

const unsigned int XkbAllMapComponentsMask = 0xff;
const unsigned int XkbUseCoreKbd = 0x0100;

typedef XkbDescPtr (*XkbGetMap)(Display *display, unsigned int which, unsigned int device_spec);

typedef struct {
  int type;
  unsigned long serial;
  Bool send_event;
  Display *display;
  Window window;
  Window root;
  Window subwindow;
  Time time;
  int x, y;
  int x_root, y_root;
  unsigned int state;
  unsigned int keycode;
  Bool same_screen;
} XKeyEvent;

typedef XKeyEvent XKeyPressedEvent;
typedef XKeyEvent XKeyReleasedEvent;

const int KeyPress = 2;
const int KeyRelease = 3;

const Time CurrentTime = 0L;

/*
typedef struct {
  int type;
  unsigned long serial;
  Bool send_event;
  Display *display;
  Window window;
  Window root;
  Window subwindow;
  Time time;
  int x, y;
  int x_root, y_root;
  unsigned int state;
  unsigned int button;
  Bool same_screen;
} XButtonEvent;

typedef XButtonEvent XButtonPressedEvent;
typedef XButtonEvent XButtonReleasedEvent;
*/

#endif

