#include "../x11/headers.h"

typedef void *GC;

typedef GC (*GetICAGC)(void);
typedef Display *(*GetICADisplay)(void);
typedef Window (*GetICADisplayWindow)(void);
typedef int (*NCSXKeyDown)(XKeyPressedEvent *event);
typedef int (*NCSXKeyUp)(XKeyReleasedEvent *event);

/*
typedef int (*NCSXMouseBtnDown)(XButtonPressedEvent *event);
typedef int (*NCSXMouseBtnUp)(XButtonReleasedEvent *event);
typedef int (*NCSXMouseMovement)(XPointerMovedEvent *event);
*/
