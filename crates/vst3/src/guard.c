/* Calls into VST 3 plug-ins under Windows' structured exception handling:
   a crash in a plug-in (an access violation, a C++ exception it lets out,
   a stack overflow) comes back to BananaV as the exception's code and
   address instead of ending the program. See guard.rs. */

#include <malloc.h>
#include <windows.h>

/* The shapes of the plug-ins' functions guarded: an object and a pointer,
   an object and a byte, an object alone; each gives a result code. */
/* COM's calling convention: stdcall on 32-bit x86 (64-bit Windows has
   one convention and ignores the keyword). */
typedef int (__stdcall *bv_pp)(void *self, void *arg);
typedef int (__stdcall *bv_pb)(void *self, unsigned char arg);
typedef int (__stdcall *bv_p)(void *self);

/* What crashed: the exception's code and the instruction's address. */
typedef struct bv_crash {
    unsigned long code;
    void *address;
} bv_crash;

static int bv_noted(EXCEPTION_POINTERS *info, bv_crash *crash) {
    crash->code = info->ExceptionRecord->ExceptionCode;
    crash->address = info->ExceptionRecord->ExceptionAddress;
    return EXCEPTION_EXECUTE_HANDLER;
}

/* After a stack overflow the guard page has to be put back for the next. */
static unsigned long bv_caught(const bv_crash *crash, void **address) {
    if (crash->code == EXCEPTION_STACK_OVERFLOW) {
        _resetstkoflw();
    }
    *address = crash->address;
    return crash->code == 0 ? 1 : crash->code;
}

/* `f(self, arg)`: 0 and its result in `result`, or the code of the
   exception it crashed with (its address in `address`). */
unsigned long bv_vst3_guard_pp(bv_pp f, void *self, void *arg, int *result, void **address) {
    bv_crash crash = {0, 0};
    __try {
        *result = f(self, arg);
    } __except (bv_noted(GetExceptionInformation(), &crash)) {
        return bv_caught(&crash, address);
    }
    return 0;
}

unsigned long bv_vst3_guard_pb(bv_pb f, void *self, unsigned char arg, int *result,
                               void **address) {
    bv_crash crash = {0, 0};
    __try {
        *result = f(self, arg);
    } __except (bv_noted(GetExceptionInformation(), &crash)) {
        return bv_caught(&crash, address);
    }
    return 0;
}

unsigned long bv_vst3_guard_p(bv_p f, void *self, int *result, void **address) {
    bv_crash crash = {0, 0};
    __try {
        *result = f(self);
    } __except (bv_noted(GetExceptionInformation(), &crash)) {
        return bv_caught(&crash, address);
    }
    return 0;
}
