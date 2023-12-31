#include "../src/ringbuf_ref.h"
#include "assert.h"
#include "stdio.h"
struct item {
    int a;
    int b;
};

struct item payload_0[7];
struct item payload_1[32];

struct RingBufRef test_rb[2] = {
    {
    0, 0, sizeof(struct item), 7, (uint8 *)payload_0
    },
    {
    0, 0, sizeof(struct item), 32, (uint8 *)payload_1
    },
};

struct ConsumerState {
    struct RingBufRef * buf;
};

struct ProducerState {
    int iter;
    struct RingBufRef * buf;
};

void consumer_isr(int irq_idx) {

    static struct ConsumerState states[2];

    struct ConsumerState * my_state = &states[irq_idx];

    if (my_state->buf == 0) {
        my_state->buf = &(test_rb[irq_idx]);
    }

    while (! is_empty(my_state->buf) ) {
        struct item * front = (struct item *)reader_front(my_state->buf);
        printf("irq %d popd %d\n", irq_idx, front->a);
        pop(my_state->buf);
    }
}

void producer_isr(int irq_idx) {

    static struct ProducerState states[2];

    struct ProducerState * my_state = &states[irq_idx];

    if (my_state->buf == 0) {
        my_state->buf = &(test_rb[irq_idx]);
    }
    
    assert( !is_full(my_state->buf));

    struct item * wf = (struct item *) writer_front(my_state->buf);

    wf->a = my_state->iter++;

    commit(my_state->buf);

}

int main() {

    for (int i=0; i<7; i++) {
        producer_isr(0);
    }
    assert(is_full(&test_rb[0]));
    consumer_isr(0);

    assert(is_empty(&test_rb[0]));

    for (int i=0; i<7; i++) {
        producer_isr(0);
    }
    assert(is_full(&test_rb[0]));
    consumer_isr(0);

    assert(is_empty(&test_rb[0]));

    return 0;

}