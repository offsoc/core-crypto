use crate::utils::{proteus_bench::*, *};
use core_crypto::prelude::{CertificateBundle, MlsCiphersuite};
use criterion::{
    BatchSize, BenchmarkId, Criterion, async_executor::AsyncStdExecutor as FuturesExecutor, black_box, criterion_group,
    criterion_main,
};

use proteus::{
    keys,
    keys::{PreKey, PreKeyBundle},
};
use rand::distributions::{Alphanumeric, DistString};

#[path = "utils/mod.rs"]
mod utils;

fn mls_cases() -> Vec<(MlsCiphersuite, Option<CertificateBundle>, bool, &'static str)> {
    // Ciphersuite 3 is the closest to proteus one
    const CIPHERSUITE: MlsTestCase = MlsTestCase::Basic_Ciphersuite1;
    let (_, ciphersuite, credential) = CIPHERSUITE.get();
    let in_memory = (ciphersuite, credential.clone(), true, "MLS/mem");
    let in_db = (ciphersuite, credential, false, "MLS/db");
    if cfg!(feature = "bench-in-db") {
        vec![in_memory, in_db]
    } else {
        vec![in_memory]
    }
}

fn proteus_cases() -> Vec<(bool, &'static str)> {
    let in_memory = (true, "Proteus/mem");
    let in_db = (false, "Proteus/db");
    if cfg!(feature = "bench-in-db") {
        vec![in_memory, in_db]
    } else {
        vec![in_memory]
    }
}

fn encrypt_message_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("Mls vs Proteus: encrypt");
    for i in (GROUP_RANGE_PROTEUS).step_by(GROUP_STEP_PROTEUS) {
        // MLS
        for (ciphersuite, credential, in_memory, bench_name) in mls_cases() {
            group.bench_with_input(BenchmarkId::new(bench_name, i), &i, |b, i| {
                b.to_async(FuturesExecutor).iter_batched(
                    || {
                        async_std::task::block_on(async {
                            let (mut central, id, delivery_service) =
                                setup_mls(ciphersuite, credential.as_ref(), in_memory).await;
                            add_clients(&mut central, &id, ciphersuite, *i, delivery_service).await;
                            let text = Alphanumeric.sample_string(&mut rand::thread_rng(), MSG_MAX);
                            (central, id, text)
                        })
                    },
                    |(central, id, text)| async move {
                        let context = central.new_transaction().await.unwrap();
                        black_box(
                            context
                                .conversation(&id)
                                .await
                                .unwrap()
                                .encrypt_message(text)
                                .await
                                .unwrap(),
                        );
                        context.finish().await.unwrap();
                    },
                    BatchSize::SmallInput,
                )
            });
        }
        // Proteus
        for (in_memory, bench_name) in proteus_cases() {
            group.bench_with_input(BenchmarkId::new(bench_name, i), &i, |b, i| {
                b.to_async(FuturesExecutor).iter_batched(
                    || {
                        async_std::task::block_on(async {
                            let central = setup_proteus(in_memory).await;
                            let context = central.new_transaction().await.unwrap();
                            let session_material = (0..*i)
                                .map(|_| (session_id(), new_prekey().serialise().unwrap()))
                                .collect::<Vec<(String, Vec<u8>)>>();
                            for (session_id, key) in &session_material {
                                context.proteus_session_from_prekey(session_id, key).await.unwrap();
                            }
                            let text = Alphanumeric.sample_string(&mut rand::thread_rng(), MSG_MAX);
                            context.finish().await.unwrap();
                            (central, session_material, text)
                        })
                    },
                    |(central, session_material, text)| async move {
                        let context = central.new_transaction().await.unwrap();
                        for (session_id, _) in session_material {
                            black_box(context.proteus_encrypt(&session_id, text.as_bytes()).await.unwrap());
                        }
                        context.finish().await.unwrap();
                    },
                    BatchSize::SmallInput,
                )
            });
        }
    }
    group.finish();
}

fn add_client_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("Mls vs Proteus: add");
    for i in (GROUP_RANGE_PROTEUS).step_by(GROUP_STEP_PROTEUS) {
        // MLS
        for (ciphersuite, credential, in_memory, bench_name) in mls_cases() {
            group.bench_with_input(BenchmarkId::new(bench_name, i), &i, |b, i| {
                b.to_async(FuturesExecutor).iter_batched(
                    || {
                        async_std::task::block_on(async {
                            let (mut central, id, delivery_service) =
                                setup_mls(ciphersuite, credential.as_ref(), in_memory).await;
                            add_clients(&mut central, &id, ciphersuite, *i, delivery_service).await;
                            let (kp, _) = rand_key_package(ciphersuite).await;
                            (central, id, vec![kp.into()])
                        })
                    },
                    |(central, id, kps)| async move {
                        let context = central.new_transaction().await.unwrap();
                        black_box(context.conversation(&id).await.unwrap().add_members(kps).await.unwrap());
                        context.finish().await.unwrap();
                        black_box(());
                    },
                    BatchSize::SmallInput,
                )
            });
        }
        // Proteus
        // From proteus POV, adding 1 client in a group of N means adding N times 1 client to N central
        // To simplify we are just going to add N times a client to just 1 central
        for (in_memory, bench_name) in proteus_cases() {
            group.bench_with_input(BenchmarkId::new(bench_name, i), &i, |b, i| {
                b.to_async(FuturesExecutor).iter_batched(
                    || {
                        async_std::task::block_on(async {
                            let central = setup_proteus(in_memory).await;
                            let session_material = (0..*i)
                                .map(|_| (session_id(), new_prekey().serialise().unwrap()))
                                .collect::<Vec<(String, Vec<u8>)>>();
                            (central, session_material)
                        })
                    },
                    |(central, session_material)| async move {
                        let context = central.new_transaction().await.unwrap();
                        for (session_id, key) in session_material {
                            black_box(context.proteus_session_from_prekey(&session_id, &key).await.unwrap());
                        }
                        context.finish().await.unwrap();
                    },
                    BatchSize::SmallInput,
                )
            });
        }
    }
    group.finish();
}

fn remove_client_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("Mls vs Proteus: remove");
    for i in (GROUP_RANGE_PROTEUS).step_by(GROUP_STEP_PROTEUS) {
        // MLS
        for (ciphersuite, credential, in_memory, bench_name) in mls_cases() {
            group.bench_with_input(BenchmarkId::new(bench_name, i), &i, |b, i| {
                b.to_async(FuturesExecutor).iter_batched(
                    || {
                        async_std::task::block_on(async {
                            let (mut central, id, delivery_service) =
                                setup_mls(ciphersuite, credential.as_ref(), in_memory).await;
                            let (client_ids, ..) =
                                add_clients(&mut central, &id, ciphersuite, GROUP_MAX_PROTEUS, delivery_service).await;
                            let to_remove = client_ids[..*i].to_vec();
                            (central, id, to_remove)
                        })
                    },
                    |(central, id, client_ids)| async move {
                        let context = central.new_transaction().await.unwrap();
                        black_box(
                            context
                                .conversation(&id)
                                .await
                                .unwrap()
                                .remove_members(client_ids.as_slice())
                                .await
                                .unwrap(),
                        );
                        context.finish().await.unwrap();
                        black_box(());
                    },
                    BatchSize::SmallInput,
                )
            });
        }
        // Proteus
        // From proteus POV, removing 1 client from a group of N means removing N times 1 client from N central
        // To simplify we are just going to remove N times a client from just 1 central
        for (in_memory, bench_name) in proteus_cases() {
            group.bench_with_input(BenchmarkId::new(bench_name, i), &i, |b, i| {
                b.to_async(FuturesExecutor).iter_batched(
                    || {
                        async_std::task::block_on(async {
                            let central = setup_proteus(in_memory).await;
                            let session_material = (0..*i)
                                .map(|_| (session_id(), new_prekey().serialise().unwrap()))
                                .collect::<Vec<(String, Vec<u8>)>>();
                            let context = central.new_transaction().await.unwrap();
                            for (session_id, key) in &session_material {
                                context.proteus_session_from_prekey(session_id, key).await.unwrap();
                            }
                            context.finish().await.unwrap();
                            (central, session_material)
                        })
                    },
                    |(central, session_material)| async move {
                        let context = central.new_transaction().await.unwrap();
                        for (session_id, _) in session_material {
                            context.proteus_session_delete(&session_id).await.unwrap();
                            black_box(());
                        }
                        context.finish().await.unwrap();
                    },
                    BatchSize::SmallInput,
                )
            });
        }
    }
    group.finish();
}

fn update_client_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("Mls vs Proteus: update");
    for i in (GROUP_RANGE_PROTEUS).step_by(GROUP_STEP_PROTEUS) {
        // MLS
        for (ciphersuite, credential, in_memory, bench_name) in mls_cases() {
            group.bench_with_input(BenchmarkId::new(bench_name, i), &i, |b, i| {
                b.to_async(FuturesExecutor).iter_batched(
                    || {
                        async_std::task::block_on(async {
                            let (mut central, id, delivery_service) =
                                setup_mls(ciphersuite, credential.as_ref(), in_memory).await;
                            add_clients(&mut central, &id, ciphersuite, *i, delivery_service).await;
                            (central, id)
                        })
                    },
                    |(central, id)| async move {
                        let context = central.new_transaction().await.unwrap();
                        black_box(
                            context
                                .conversation(&id)
                                .await
                                .unwrap()
                                .update_key_material()
                                .await
                                .unwrap(),
                        );
                        context.finish().await.unwrap();
                        black_box(());
                    },
                    BatchSize::SmallInput,
                )
            });
        }
        // Proteus
        // From proteus POV, adding 1 client in a group of N means adding N times 1 client to N central
        // To simplify we are just going to add N times a client to just 1 central
        for (in_memory, bench_name) in proteus_cases() {
            group.bench_with_input(BenchmarkId::new(bench_name, i), &i, |b, i| {
                b.to_async(FuturesExecutor).iter_batched(
                    || {
                        async_std::task::block_on(async {
                            let central = setup_proteus(in_memory).await;
                            let session_material = (0..*i)
                                .map(|_| (session_id(), new_prekey().serialise().unwrap()))
                                .collect::<Vec<(String, Vec<u8>)>>();
                            let context = central.new_transaction().await.unwrap();
                            for (session_id, key) in &session_material {
                                context.proteus_session_from_prekey(session_id, key).await.unwrap();
                            }
                            context.finish().await.unwrap();
                            let new_pkb = PreKeyBundle::new(
                                keys::IdentityKeyPair::new().public_key,
                                &PreKey::new(keys::PreKeyId::new(2)),
                            )
                            .serialise()
                            .unwrap();
                            (central, new_pkb, session_material)
                        })
                    },
                    |(central, new_pkb, session_material)| async move {
                        let context = central.new_transaction().await.unwrap();
                        for (session_id, _) in session_material {
                            // replace existing session
                            context.proteus_session_delete(&session_id).await.unwrap();
                            black_box(());
                            black_box(
                                context
                                    .proteus_session_from_prekey(&session_id, &new_pkb)
                                    .await
                                    .unwrap(),
                            );
                        }
                        context.finish().await.unwrap();
                    },
                    BatchSize::SmallInput,
                )
            });
        }
    }
    group.finish();
}

criterion_group!(
    name = mls_vs_proteus;
    config = criterion();
    targets =
    encrypt_message_bench,
    add_client_bench,
    remove_client_bench,
    update_client_bench,
);
criterion_main!(mls_vs_proteus);
