use detersim_net::{connect_pair, ConnectionId, StreamFault};

#[test]
fn stream_delivers_frames_in_sequence_order() {
    let mut stream = connect_pair(0, 1, ConnectionId(7));
    stream.send(b"a".to_vec(), &[]);
    stream.send(
        b"b".to_vec(),
        &[
            StreamFault::Duplicate { seq: 1 },
            StreamFault::Delay {
                seq: 1,
                after_seq: 0,
            },
        ],
    );
    let transcript = stream.into_transcript();

    assert_eq!(
        transcript
            .delivered
            .iter()
            .map(|frame| String::from_utf8_lossy(&frame.bytes).to_string())
            .collect::<Vec<_>>(),
        vec!["a".to_string(), "b".to_string()]
    );
    assert!(transcript
        .events
        .iter()
        .any(|event| event.label == "stream:duplicate:seq=1"));
}

#[test]
fn stream_faults_are_visible_in_transcript() {
    let mut stream = connect_pair(0, 1, ConnectionId(8));
    stream.send(b"a".to_vec(), &[StreamFault::Drop { seq: 0 }]);
    stream.send(b"b".to_vec(), &[StreamFault::Disconnect]);
    stream.send(b"c".to_vec(), &[StreamFault::Reconnect]);
    let transcript = stream.into_transcript();

    assert!(transcript
        .to_history_lines()
        .contains(&"stream:drop:seq=0".to_string()));
    assert!(transcript
        .to_history_lines()
        .contains(&"stream:disconnect".to_string()));
    assert!(transcript
        .to_history_lines()
        .contains(&"stream:blocked:seq=1".to_string()));
}
