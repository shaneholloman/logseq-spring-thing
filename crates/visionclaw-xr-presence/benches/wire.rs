use criterion::{black_box, criterion_group, criterion_main, Criterion};
use visionclaw_xr_presence::types::{Aabb, PoseFrame};
use visionclaw_xr_presence::{
    decode, encode, validate, AvatarId, Did, PoseDelta, RoomId, Transform,
};

fn fixture() -> (RoomId, AvatarId, PoseFrame) {
    let room = RoomId::parse("urn:visionclaw:room:sha256-12-deadbeefcafe").unwrap();
    let did = Did::parse(format!("did:nostr:{}", "a".repeat(64))).unwrap();
    let avatar = AvatarId::from_did(&did);
    let t = Transform {
        position: [1.0, 1.7, -0.5],
        rotation: [0.0, 0.0, 0.0, 1.0],
    };
    let frame = PoseFrame {
        timestamp_us: 1_700_000_000_000_000,
        head: t,
        left_hand: Some(t),
        right_hand: Some(t),
    };
    (room, avatar, frame)
}

fn fixture_pair() -> (PoseFrame, PoseFrame) {
    let (_, _, prev) = fixture();
    let mut next = prev.clone();
    next.timestamp_us += 11_111;
    next.head.position[0] += 0.05;
    next.head.position[2] -= 0.02;
    if let Some(ref mut lh) = next.left_hand {
        lh.position[1] += 0.01;
    }
    (prev, next)
}

fn bench_encode(c: &mut Criterion) {
    let (room, avatar, frame) = fixture();
    c.bench_function("encode_pose_frame", |b| {
        b.iter(|| encode(black_box(&frame), black_box(&room), black_box(&avatar)).unwrap())
    });
}

fn bench_decode(c: &mut Criterion) {
    let (room, avatar, frame) = fixture();
    let bytes = encode(&frame, &room, &avatar).unwrap();
    c.bench_function("decode_pose_frame", |b| {
        b.iter(|| decode(black_box(&bytes)).unwrap())
    });
}

fn bench_validate_pose(c: &mut Criterion) {
    let (prev, next) = fixture_pair();
    let bounds = Aabb::symmetric(50.0);
    c.bench_function("validate_pose", |b| {
        b.iter(|| {
            validate::velocity_gate(black_box(&prev), black_box(&next), 20.0).unwrap();
            validate::world_bounds(black_box(&next.head), black_box(&bounds)).unwrap();
            validate::monotonic_timestamp(black_box(prev.timestamp_us), black_box(next.timestamp_us))
                .unwrap();
        })
    });
}

fn bench_delta_compute(c: &mut Criterion) {
    let (prev, next) = fixture_pair();
    c.bench_function("delta_compute", |b| {
        b.iter(|| PoseDelta::between(black_box(&prev), black_box(&next)))
    });
}

criterion_group!(
    benches,
    bench_encode,
    bench_decode,
    bench_validate_pose,
    bench_delta_compute
);
criterion_main!(benches);
