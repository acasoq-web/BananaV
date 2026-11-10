/* Calls into plug-ins behind a guard on macOS (see guard.rs): a crash in a
   plug-in — a bad memory access, an illegal instruction, a C++ exception
   it lets out (std::terminate, then abort) — comes back to BananaV as the
   signal and the address of the instruction instead of ending the
   program. The handlers are installed before any plug-in's code is
   loaded: a plug-in that installs its own later is asked first, as it
   expects; a signal outside a guarded call goes to the handler that was
   there before (Rust's), or does what the system does by default. */

#include <pthread.h>
#include <setjmp.h>
#include <signal.h>
#include <stddef.h>
#include <string.h>
#include <sys/ucontext.h>

typedef void (*bv_body)(void *context);

/* The guard of the call running on this thread, and what stopped it. */
static __thread sigjmp_buf *bv_jump;
static __thread int bv_signal;
static __thread void *bv_where;

static struct sigaction bv_before[32];
static pthread_once_t bv_once = PTHREAD_ONCE_INIT;
static const int bv_signals[] = {SIGSEGV, SIGBUS, SIGILL, SIGFPE, SIGABRT, SIGTRAP};

/* The instruction that stopped (else the address it touched). */
static void *bv_instruction(siginfo_t *info, void *context) {
    ucontext_t *uc = (ucontext_t *)context;
#if defined(__aarch64__) || defined(__arm64__)
    if (uc != NULL && uc->uc_mcontext != NULL) {
        return (void *)uc->uc_mcontext->__ss.__pc;
    }
#elif defined(__x86_64__)
    if (uc != NULL && uc->uc_mcontext != NULL) {
        return (void *)uc->uc_mcontext->__ss.__rip;
    }
#endif
    return info != NULL ? info->si_addr : NULL;
}

static void bv_handler(int sig, siginfo_t *info, void *context) {
    sigjmp_buf *jump = bv_jump;
    if (jump != NULL) {
        bv_jump = NULL;
        bv_signal = sig;
        bv_where = bv_instruction(info, context);
        siglongjmp(*jump, 1);
    }
    struct sigaction *before = &bv_before[sig];
    if (before->sa_flags & SA_SIGINFO) {
        if (before->sa_sigaction != NULL) {
            before->sa_sigaction(sig, info, context);
            return;
        }
    } else if (before->sa_handler == SIG_IGN) {
        return;
    } else if (before->sa_handler != SIG_DFL && before->sa_handler != NULL) {
        before->sa_handler(sig);
        return;
    }
    /* The system's own action: a fault happens again on return and ends
       the program; abort's signal is raised again. */
    signal(sig, SIG_DFL);
    if (sig == SIGABRT) {
        raise(sig);
    }
}

static void bv_install(void) {
    struct sigaction sa;
    memset(&sa, 0, sizeof sa);
    sa.sa_sigaction = bv_handler;
    sa.sa_flags = SA_SIGINFO | SA_ONSTACK;
    sigemptyset(&sa.sa_mask);
    for (size_t i = 0; i < sizeof bv_signals / sizeof bv_signals[0]; i++) {
        sigaction(bv_signals[i], &sa, &bv_before[bv_signals[i]]);
    }
}

void bv_apple_guard_install(void) {
    pthread_once(&bv_once, bv_install);
}

/* `body(context)`: 0, or the signal it stopped with (the instruction's
   address in `address`). Guards nest: an inner one gives the outer back. */
unsigned long bv_apple_guarded(bv_body body, void *context, void **address) {
    sigjmp_buf jump;
    sigjmp_buf *volatile outer = bv_jump;
    bv_apple_guard_install();
    if (sigsetjmp(jump, 1) == 0) {
        bv_jump = &jump;
        body(context);
        bv_jump = outer;
        return 0;
    }
    bv_jump = outer;
    *address = bv_where;
    return (unsigned long)bv_signal;
}
