/* Context inheritance and resumption, using native frames as the existing
 * shared-object probe does. Run on one driver so a context entered in a
 * child account must relinquish that thread to a sibling before resuming. */
#define _GNU_SOURCE
#include "bridge.h"
#include "../ordinary_values.h"
#include "../sched/core.h"
#include "../runtime_test_guard.h"
#include <string.h>

#define CHECK(test) do { if (!(test)) { \
    fprintf(stderr, "scope-context-probe:%d: %s\n", __LINE__, #test); exit(1); \
} } while (0)

typedef struct {
    unsigned kind, step, done;
    uint64_t group[2];
    void *block;
} scope_frame;

static wf_value meter, parent, child, child_view, parent_view;
static unsigned child_entered, sibling_ran;

static void close_owner(const wf_value *owner) {
    wf_scope_close_result result;
    wf__body_scope_close(&result, &meter, owner);
    CHECK(result.tag == 0u);
}

static void read_bytes(const wf_value *view, uint64_t bytes) {
    wf_optional_bytes result;
    wf__body_scope_bytes(&result, view);
    CHECK(result.tag == WF_OPTION_SOME && result.value == bytes);
}

static void *start(void *argument) {
    scope_frame *frame = wf__context_frame_allocate(sizeof(*frame));
    memset(frame, 0, sizeof(*frame));
    frame->kind = *(unsigned *)argument;
    CHECK(wf__scope_current() == parent.words[0]);
    return frame;
}

static void spawn(scope_frame *root, unsigned kind) {
    unsigned *argument = wf__context_prepare(sizeof(*argument));
    *argument = kind;
    wf__context_launch(root->group, argument, start);
    CHECK(wf__scope_current() == parent.words[0]);
}

void wf__coro_resume(void *opaque) {
    scope_frame *frame = opaque;
    if (frame->kind == 0u) {
        if (frame->step == 0u) {
            CHECK(wf__scope_current() == 0u);
            CHECK(wf__body_scope_enter(&parent));
            spawn(frame, 1u);
            spawn(frame, 2u);
            frame->step = 1u;
            CHECK(wf__context_join_wait(frame->group, frame));
            return;
        }
        CHECK(wf__scope_current() == parent.words[0]);
        CHECK(child_entered && sibling_ran);
        CHECK(!wf__context_join_wait(frame->group, frame));
        read_bytes(&child_view, 0u);
        read_bytes(&parent_view, 0u); /* Joined context pool grants returned. */
        close_owner(&child);
        wf__body_scope_leave(&parent);
        CHECK(wf__scope_current() == 0u);
        close_owner(&parent);
        frame->done = 1u;
    } else if (frame->kind == 1u) {
        if (frame->step == 0u) {
            CHECK(wf__scope_current() == parent.words[0]);
            CHECK(wf__body_scope_enter(&child));
            frame->block = wf__heap_take(19u);
            CHECK(frame->block != NULL);
            child_entered = 1u;
            frame->step = 1u;
        }
        CHECK(wf__scope_current() == child.words[0]);
        while (!sibling_ran) {
            if (wf__context_pass(frame)) return;
        }
        read_bytes(&child_view, 19u);
        wf__heap_give(frame->block, 19u);
        wf__body_scope_leave(&child);
        CHECK(wf__scope_current() == parent.words[0]);
        frame->done = 1u;
    } else {
        CHECK(wf__scope_current() == parent.words[0]);
        while (!child_entered) {
            if (wf__context_pass(frame)) return;
        }
        read_bytes(&child_view, 19u);
        void *block = wf__heap_take(5u);
        CHECK(block != NULL);
        wf__heap_give(block, 5u);
        sibling_ran = 1u;
        frame->done = 1u;
    }
}

void wf__coro_destroy(void *frame) { wf__context_frame_release(frame); }
int wf__coro_done(void *frame) { return ((scope_frame *)frame)->done != 0u; }

int main(void) {
    CHECK(setenv("WF_DRIVERS", "1", 1) == 0);
    wf_test_guard_start(30);
    wf_scope_open_result opened;
    wf__body_scope_open(&opened, &meter);
    CHECK(opened.tag == 0u);
    parent = opened.ok.value;
    wf__body_scope_open_child(&opened, &meter, &parent);
    CHECK(opened.tag == 0u);
    child = opened.ok.value;
    wf__body_scope_view(&parent_view, &parent);
    wf__body_scope_view(&child_view, &child);
    wf__context_root_begin();
    scope_frame *root = wf__context_frame_allocate(sizeof(*root));
    memset(root, 0, sizeof(*root));
    wf__context_root_run(root);
    CHECK(wf__scope_current() == 0u);
    wf_optional_bytes closed;
    wf__body_scope_bytes(&closed, &child_view);
    CHECK(closed.tag == 0u);
    wf__body_scope_bytes(&closed, &parent_view);
    CHECK(closed.tag == 0u);
    wf_test_guard_finish();
    puts("scope-context-probe: PASS");
    return 0;
}
