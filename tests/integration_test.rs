use spsc_ringbuf_core::shared_pool::*;
const POOL_DEPTH: usize = 16;
pub struct Message {
    id: u32,
    payload: PoolIndex<POOL_DEPTH>,
}

impl HasPoolIdx<POOL_DEPTH> for Message {
    fn get_pool_idx(&self) -> PoolIndex<POOL_DEPTH> {
        self.payload
    }
    fn set_pool_idx(&mut self, pindex: PoolIndex<POOL_DEPTH>) {
        self.payload = pindex
    }
}

pub struct Payload {
    value: u32,
}

// Declare a static version
static SHARED_POOL: SharedPool<Payload, Message, 16, 32> = SharedPool::new();

#[test]
fn test_errors() {

    // 32 deep ring buffer and 16-deep payload pool
    let shared_pool: SharedPool<Payload, Message, 16, 32> = SharedPool::new();

    // Split producer and consumer objects in one shot
    let (mut producer, mut consumer) =  shared_pool.split().unwrap();

    // stage the write location for write. This is what we called as "stage"
    // This is staging without payload
    let message = producer.stage().unwrap();

    // Write something to the message itself
    message.id = 41;

    // Commit the message
    assert!(producer.commit().is_ok());

    let (recvd, payload) = consumer.peek_with_payload();

    // Consumer side should be able to peek it now
    let recvd = recvd.unwrap();

    // Assert that there's no payload
    assert!(!recvd.get_pool_idx().is_valid());

    // There's no payload
    assert!(payload.is_none());
    
    // Return an invalid location will assert!
    //assert!(consumer.enqueue_return(recvd.get_pool_idx()).is_err());

    // There also no way to get the raw index as it's private
    //let pidx = recvd.get_pool_idx();
    //pidx.0 = 2;

    // Pop the message
    consumer.pop().unwrap();

    // Try to use up all the payloads
    for i in 0..16 {

        let (_, payload) = producer.stage_with_payload().unwrap();
        // unstageed to producer
        let inner = payload.try_write().unwrap();
        inner.value = i;
        // Mark the payload as ready for consumer
        payload.write_done().unwrap();
        // Commit the message to consumer
        producer.commit().unwrap();

    }

    // No way to stage one more as everything has been 
    // allocated
    assert!(producer.stage_with_payload().is_err());

    // Get the first one in queue, it must have payload
    let (recvd, payload) = consumer.peek_with_payload();

    let recvd = recvd.unwrap();
    let payload = payload.unwrap();

    // Copy the pool idx for return purpose
    let pool_idx = recvd.get_pool_idx();

    // Private, note modifiable pool_idx.0 = 2;
    // Private, cannot be created.
    // let payload_idx = PoolIndex::<16>(1);

    // Return the payload location
    payload.read_done().unwrap();

    // Return the message
    consumer.pop().unwrap();

    // Staging with payload should still fail since the payload pool is still empty
    assert!(producer.stage_with_payload().is_err());

    // stage without payload should be fine
    assert!(producer.stage().is_some());

    // Return the index
    assert!(consumer.return_payload(pool_idx).is_ok());

    // Should be possible to stage with payload
    let new_stage = producer.stage_with_payload();

    match new_stage {
        Ok((msg, _)) => assert!(msg.get_pool_idx().is_valid()),
        _ => panic!("new stage should have valid payload!") 
    }

}

#[test]
fn test_threads() {

    use std::thread;
    use std::time::Duration;

    // 32 deep ring buffer and 16-deep payload pool
    let (mut producer, mut consumer) =  SHARED_POOL.split().unwrap();

    let total_transfer = 200;
    
    let c_handle = thread::spawn(move || {

        let mut exit = false;
        let mut expected = 0;
        while !exit {

            if consumer.peek().is_some() {

                let (recvd, payload) = consumer.peek_with_payload();

                let recvd = recvd.unwrap();

                let payload = payload.unwrap();

                // Copy the pool idx for return purpose
                let pool_idx = recvd.get_pool_idx();
                
                assert!(payload.try_read().unwrap().value == expected);

                println!("consume {}", expected);

                expected += 1;

                if payload.try_read().unwrap().value == total_transfer - 1 {
                    exit = true;
                }
                // Return the payload location
                payload.read_done().unwrap();

                // Return the message
                consumer.pop().unwrap();

                // Return the pool location
                assert!(consumer.return_payload(pool_idx).is_ok());

            }
            thread::sleep(Duration::from_millis(10));

        }
    });

    let p_handle = thread::spawn(move || {

        let mut exit = false;
        let mut count = 0;
        while !exit {
            thread::sleep(Duration::from_millis(1));

            if let Ok((_, payload)) =  producer.stage_with_payload(){
                let inner = payload.try_write().unwrap();
                inner.value = count;
                println!("produce {}", count);
                count += 1;

                // Mark the payload as ready for consumer
                payload.write_done().unwrap();
                // Commit the message to consumer
                producer.commit().unwrap();

                if count == total_transfer {
                    exit = true;
                }
            }
        }
    });

    p_handle.join().unwrap();
    c_handle.join().unwrap();
}


