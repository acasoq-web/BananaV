/* Calls into VST 2 plug-ins under Windows' structured exception handling:
   a crash in a plug-in (an access violation, a C++ exception it lets out,
   a stack overflow) comes back to BananaV as the exception's code and
   address instead of ending the program. See guard.rs. The plug-ins'
   functions go by the C calling convention (cdecl on 32-bit x86). */

#include <malloc.h>
#include <stdint.h>
#include <windows.h>

typedef intptr_t (__cdecl *bv_dispatch)(void *plugin, int request, int index, intptr_t value,
                                        void *ptr, float number);
typedef void (__cdecl *bv_process)(void *plugin, float **inputs, float **outputs, int frames);
typedef void (__cdecl *bv_set)(void *plugin, int index, float value);
typedef float (__cdecl *bv_get)(void *plugin, int index);
typedef void *(__cdecl *bv_entry)(void *host);

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

/* Each: 0 and the result, or the code of the exception (its address in
   `address`). */

unsigned long bv_vst2_dispatch(bv_dispatch f, void *plugin, int request, int index,
                               intptr_t value, void *ptr, float number, intptr_t *result,
                               void **address) {
    bv_crash crash = {0, 0};
    __try {
        *result = f(plugin, request, index, value, ptr, number);
    } __except (bv_noted(GetExceptionInformation(), &crash)) {
        return bv_caught(&crash, address);
    }
    return 0;
}

unsigned long bv_vst2_process(bv_process f, void *plugin, float **inputs, float **outputs,
                              int frames, void **address) {
    bv_crash crash = {0, 0};
    __try {
        f(plugin, inputs, outputs, frames);
    } __except (bv_noted(GetExceptionInformation(), &crash)) {
        return bv_caught(&crash, address);
    }
    return 0;
}

unsigned long bv_vst2_set(bv_set f, void *plugin, int index, float value, void **address) {
    bv_crash crash = {0, 0};
    __try {
        f(plugin, index, value);
    } __except (bv_noted(GetExceptionInformation(), &crash)) {
        return bv_caught(&crash, address);
    }
    return 0;
}

unsigned long bv_vst2_get(bv_get f, void *plugin, int index, float *result, void **address) {
    bv_crash crash = {0, 0};
    __try {
        *result = f(plugin, index);
    } __except (bv_noted(GetExceptionInformation(), &crash)) {
        return bv_caught(&crash, address);
    }
    return 0;
}

unsigned long bv_vst2_entry(bv_entry f, void *host, void **result, void **address) {
    bv_crash crash = {0, 0};
    __try {
        *result = f(host);
    } __except (bv_noted(GetExceptionInformation(), &crash)) {
        return bv_caught(&crash, address);
    }
    return 0;
}
