//! Fixed capacity Single Producer Single Consumer Ringbuffer with no mutex protection.
//! Implementation based on https://www.snellman.net/blog/archive/2016-12-13-ring-buffers/
typedef unsigned char boolean;
typedef unsigned char uint8;
typedef unsigned long uint32;
#define STATIC_INLINE static inline 

/// Internal Index struct emcapsulating masking and wrapping operations
/// according to size const size N. Note that we deliberately use u32
/// to limit the index to 4 bytes and max supported capacity to 2^31-1
struct Index {
    uint32 cell;
};


enum ErrCode {
    Ok=0,
    BufFull=-1,
    BufEmpty=-2
};


STATIC_INLINE boolean is_power_of_two(uint32 x) {

    return x!=0 && (x&(x-1)) == 0;
}
//impl<const N: usize> Index<N> {

    //const OK: () = assert!(N < (u32::MAX/2) as usize, "Ringbuf capacity must be < u32::MAX/2");

    
    STATIC_INLINE void wrap_inc(struct Index* idx, const uint32 N) {

        uint32 n = N;
        // Wrapping increment by 1 first
        unsigned val = idx->cell+=1;

        // Wrap index between [0, 2*N-1]
        // For power 2 of values, the natural overflow wrap
        // matches the wraparound of N. Hence the manual wrap
        // below is not required for power of 2 N
        if ( !is_power_of_two(n) && val > 2 * n - 1) {
            // val = val - 2*N
            idx->cell -= 2*n; //val.wrapping_sub(2 * n));
        } else {
            idx->cell = val;
        }
    }
    
    
    STATIC_INLINE uint32 wrap_dist(struct Index *idx, struct Index *val, const uint32 N) {
        
        uint32 n = N;
        // If N is power of two, just return wrapp_sub(val)
        // If N is not power of two, wrap value between [0, 2*N-1]
        // Assumes current value is in the range of [-2*N, 4*N-1]
        // Not asserting here since we only take Index, which cannot be
        // incremented beyong 2*N-1
        uint32 raw = idx->cell - val->cell;
        if (!is_power_of_two(n)) {
            if ((int)raw < 0) {
                return raw + (2 * n);
            } else if (raw > 2 * n - 1) {
                return raw - (2 * n);
            }
        }
        return raw;
    }

    // Mask the value for indexing [0, N-1]
   STATIC_INLINE uint32 mask(struct Index *idx, const uint32 N) {
        uint32 n = N;
        uint32 val = idx->cell;
        if (is_power_of_two(n)) {
            return val & (n - 1);
        } else if (val > n - 1) {
            return val - n;
        } else {
            return val;
        }
    }

//}

/// A ring buffer of capacity N holding items of type T.
/// Non power-of-two N is supported but less efficient.
struct RingBufRef {
    // this is from where we dequeue items
    struct Index rd_idx;
    //  where we enqueue new items
    struct Index wr_idx;

    const int item_sz_bytes;

    const int N;

    uint8 * const buffer_ucell;
};

// Initialize like this
//uint8 payload[4*16];
//struct RingBufRef buf = {
//    0, 0, 4, 16, payload
//};

//impl<T, const N: usize> RingBufRef<T, N> {
    
    STATIC_INLINE boolean is_empty(struct RingBufRef * buf)  {
        return buf->rd_idx.cell == buf->wr_idx.cell;
    };

    STATIC_INLINE uint32 len(struct RingBufRef * buf) {
        // returns the number of elements between read and write pointer
        // use wrapping sub
        return wrap_dist(&buf->wr_idx, &buf->rd_idx, buf->N);
    };

    STATIC_INLINE boolean is_full(struct RingBufRef *buf) {
        return len(buf) == buf->N;
    };

    STATIC_INLINE uint32 capacity(struct RingBufRef * buf) {
        return buf->N;
    };

    /// Returns the write index location as mutable reference.
    /// The Result<> return enforces handling of return type
    /// I.e. if user does not check for push success, the compiler
    /// generates warnings
    /// Calling stage twice without commit in between results in the same
    /// location written! We could add some protection by remembering this
    /// during alloc but this will incur runtime cost
    STATIC_INLINE uint8 * writer_front(struct RingBufRef * buf) {
        if (!is_full(buf)) {
            // buffer_ucell contains UnsafeCell<MaybeUninit<T>>
            // UnsafeCell's get is defined as "fn get(&self) -> *mut T"
            return &buf->buffer_ucell[mask(&buf->wr_idx, buf->N)*buf->item_sz_bytes];
        } else {
            return 0;
        }
    }
    /// Commit whatever at the write index location by moving the write index
    STATIC_INLINE enum ErrCode commit(struct RingBufRef * buf) {
        if (!is_full(buf)) {
            wrap_inc(&buf->wr_idx, buf->N);
            return Ok;
        } else {
            return BufFull;
        }
    }

    /// Alloc and commit in one step by providing the value T to be written
    /// val's ownership is moved. (Question: it seems if T implements Clone,
    /// compiler copies T)
    STATIC_INLINE enum ErrCode push(struct RingBufRef *buf, uint8 *val_bytes) {
        if (!is_full(buf)) {
            // buffer_ucell contains UnsafeCell<MaybeUninit<T>>
            // UnsafeCell's get is defined as "fn get(&self) -> *mut T"
            // * (* mut T) deference allows the MaybeUninit.write() to be called to
            // Set the value
            int wr_loc = mask(&buf->wr_idx, buf->N);
            for (int i=0; i< buf->item_sz_bytes; i++) {
                buf->buffer_ucell[wr_loc*buf->item_sz_bytes+i] = val_bytes[i];
            }
            wrap_inc(&buf->wr_idx, buf->N);
            return Ok;
        } else {
            return BufFull;
        }
    }
    /// Returns an Option of reference to location at read index
    STATIC_INLINE uint8 * reader_front(struct RingBufRef * buf) {
        if (is_empty(buf)) {
            return 0;
        } else {
           return &buf->buffer_ucell[mask(&buf->rd_idx, buf->N)*buf->item_sz_bytes];
        }
    }
    STATIC_INLINE uint8 * reader_front_mut(struct RingBufRef * buf) {
        return reader_front(buf);
    }

    /// Consume the item at rd_idx
    STATIC_INLINE enum ErrCode pop(struct RingBufRef * buf) {
        if (!is_empty(buf)) {
            wrap_inc(&buf->rd_idx, buf->N);
            return Ok;
        } else {
            return BufEmpty;
        }
    }
//}