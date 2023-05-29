# Introduction

This is a heapless single producer single consumer (SPSC) ring buffer that
depends only on the Rust Core library. The implementation consists of an inner
ring buffer and a lightweight wrapper for providing a pair of mutable access
handlers that can be moved to their corresponding producer and consumer
entities. The inner ring buffer is designed to be easily instantiable as global
static encapsulating any need of using `unsafe` to access the structure in user
code.

# Basic ring buffer

The underlying ring buffer utilizes the "Array + two unmasked indices" method
illustrated
[here](https://www.snellman.net/blog/archive/2016-12-13-ring-buffers/).
Essentially this allows usage of all elements allocated in the buffer without
resorting to storing length along with read and write indices. If the Const
Generics parameter capacity `N` is power of two, the overhead of masking read
and write indices (within `[0, N-1]`) is only required upon buffer access.
If `N` is not a power of two, the implementation wraps the indices between `0`
and `2*N-1` as they are incremented. 

The maximum supported size of buffer is `2^32 - 1` items. This limit comes from
the size of the read and write indices (`u32`). This can be relaxed if needed
by changing the index to `usize` type.

The ring buffer item is a generic structure with no trait requirements.

## Usage model

The producer calls `write_front()` to beginning writing to the location under
`buffer[mask(wr_idx)]`. This returns a mutable reference to the underlying
item. After the location is populated, the producer calls `commit()` to advance
the write index. The consumer can use `read_front()` to check (and read) new
items available. The consumer uses `pop()` to consume the item and advance the
read index.

# Working around limitations of static global

In order to achieve the goal of allocating these queues statically while avoid
requiring all code that uses the structure to be wrapped in `unsafe`, the
implementation employs the following methods:

1. `Cell` wrapped read and write indices for providing interior mutability of
   indices
2. limited internal `unsafe` code to return mutable references of the inner
   buffer. This is considered safe due to the single producer and single
   consumer premise. i.e. write index is only modified by the producer and read
   index only modified by the consumer.
3. Use of `MaybeUninit` in inner buffer to avoid static initialization (not
   required in queues)
4. `Cell` structures are not inheritently thread safe and globals containing
   Cells cannot be instantiated for the lack of the `Sync` trait marker. Again
   due to the SPSC premise, the `core::marker::Sync` is added to the `RingbufRef`
   structure. Straightly speaking though, the marker should be *only* added to
   wrapper `Ringbuf` structure which enforces SPSC use with the `split`
   operation. But this is not done to allow instantianting global `RingbufRef`
   without incurring the handler overhead should the user decides the usage is
   safe enouhg.

# Top level wrapper

The top level `RingBuf` structure provides the final protection of unintended
direct usage of the inner ring buffer. This wrapper provides a one-time call
returning a pair of (mutable) handles that can be moved to producer and
consumer entities. Since the `writ_front`, `commit` and `pop` functions all
require mutable `self`, Rust's single mutable reference check should guarantee
that only a single producer or consumer is possible.

# Shared Singleton

This crate also provides a separate cheaper implementation for the special case
of single item ring buffer. In this version a tri-state `owner` flag replaces
all read and write indices manipulation overhead. This structure can also be
used in conjunction with the ring buffer to serve as ringbuffer data
extension. In a heapless environment, for example, one can imagine with dynamic
allocation it would be difficult to create a command queue with a deeper depth
than available command "payloads" since not all commands require a payload. 

In the included `SharedPool` example, payload can be allocated from a pool of
such shared singletons with pool index passed by command in the queue. The
`owner` flag provides separate ownership tracking of each item in the payload
pool. On initialization, the return ring buffer is filled to indicate that
every pool item belongs to the allocation producer. Producer allocates an item
in the pool and pass the index along with any other information through the
`alloc_prod` ring buffer. As the consumer is done with the allocated item, it
is returned (in the form of the pool index) through the return ring buffer.

```
                        Pool of SharedSingleton<T>
                        ┌─┬─┬─┬─┐   ┌───┐
                        │0│1│2│3│ ..│N-1│
                        └─┴─┴─┴┬┘   └───┘
┌────────────────┐             │PoolIndex<N>       ┌────────────────┐
│Producer        │         ┌───┘                   │Consumer        │
│ ┌───────────┐  │         │                       │ ┌───────────┐  │
│ │alloc_prod ├──┼───┐  ┌─┬┴┬─┬─┬─┐   ┌───┐     ┌──► │alloc_cons │  │
│ └───────────┘  │   └─►│0│1│2│3│4│.. │M-1├─────┘  │ └───────────┘  │
│  RingbufProducer      └─┴─┴─┴─┴─┘   └───┘        │                │
│ ┌───────────┐  │   Prod. -> Cons Ringbuf<Q,M>    │ ┌───────────┐  │
│ │return_cons│◄─┼──┐                            ┌─┼─┤return_prod│  │
│ └───────────┘  │  │   ┌─┬─┬─┬─┬─┐   ┌───┐      │ │ └───────────┘  │
│ RingbufConsumer│  └───┤0│1│2│3│4│.. │M-1│◄─────┘ │                │
└────────────────┘      └─┴─┴─┴─┴─┘   └───┘        └────────────────┘
                    Cons. -> Prod. Ringbuf<Q,M>


```
