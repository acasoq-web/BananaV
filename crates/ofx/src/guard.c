/* Calls into OpenFX plug-ins under Windows' structured exception handling:
   a crash in a plug-in (an access violation, a C++ exception it lets out,
   a stack overflow) comes back to BananaV as the exception's code and
   address instead of ending the program. See guard.rs. */

#include <malloc.h>
#include <windows.h>

typedef int (*bv_entry)(const char *action, const void *handle, void *in_args, void *out_args);
typedef void (*bv_thread)(unsigned int index, unsigned int max, void *arg);

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

/* A plug-in's entry point for `action`: 0 and its status in `status`, or
   the code of the exception it crashed with (its address in `address`). */
unsigned long bv_ofx_guarded_entry(bv_entry f, const char *action, const void *handle,
                                   void *in_args, void *out_args, int *status,
                                   void **address) {
    bv_crash crash = {0, 0};
    __try {
        *status = f(action, handle, in_args, out_args);
    } __except (bv_noted(GetExceptionInformation(), &crash)) {
        return bv_caught(&crash, address);
    }
    return 0;
}

/* A thread function a plug-in gave the multi-thread suite: 0, or the code
   of the exception it crashed with (its address in `address`). */
unsigned long bv_ofx_guarded_thread(bv_thread f, unsigned int index, unsigned int max, void *arg,
                                    void **address) {
    bv_crash crash = {0, 0};
    __try {
        f(index, max, arg);
    } __except (bv_noted(GetExceptionInformation(), &crash)) {
        return bv_caught(&crash, address);
    }
    return 0;
}
