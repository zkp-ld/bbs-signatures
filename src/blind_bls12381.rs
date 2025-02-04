/*
 * Copyright 2020 - MATTR Limited
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *     http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
use crate::{gen_signature_message, prelude::BlsKeyPair, utils::set_panic_hook, BbsVerifyResponse};

use bbs::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    convert::TryInto,
};
use wasm_bindgen::prelude::*;

wasm_impl!(
    BlindBlsSignatureRequestContextRequest,
    signerPublicKey: DeterministicPublicKey,
    proverSecretKey: Vec<u8>,
    messageCount: usize,
    nonce: Vec<u8>
);

wasm_impl!(
    BlindBlsSignatureRequestContextResponse,
    commitment: Commitment,
    proofOfHiddenMessages: ProofG1,
    challengeHash: ProofChallenge,
    blindingFactor: SignatureBlinding
);

wasm_impl!(
    BlindBlsSignatureVerifyContextRequest,
    commitment: Commitment,
    proofOfHiddenMessages: ProofG1,
    challengeHash: ProofChallenge,
    messageCount: usize,
    publicKey: DeterministicPublicKey,
    nonce: Vec<u8>
);

wasm_impl!(
    BlindBlsSignContextRequest,
    keyPair: BlsKeyPair,
    messages: Vec<Vec<u8>>,
    commitment: Commitment
);

wasm_impl!(
    UnblindBlindSignatureRequest,
    signature: BlindSignature,
    blindingFactor: SignatureBlinding
);

// inspired by bbs_blind_signature_commitment
// this fn is stricted to be able to create only one blinded message (prover secret key)
#[wasm_bindgen(js_name = blindBlsSignatureRequest)]
pub async fn blind_bls_signature_request(request: JsValue) -> Result<JsValue, JsValue> {
    set_panic_hook();
    let request: BlindBlsSignatureRequestContextRequest = request.try_into()?;

    // create (MessageCount + 1) pubkeys for messages and one proverSecretKey
    let msg_pvsk_total = request.messageCount + 1;
    let pk_res = request.signerPublicKey.to_public_key(msg_pvsk_total);
    let pk;
    match pk_res {
        Err(_) => return Err(JsValue::from_str("Failed to convert key")),
        Ok(p) => pk = p,
    };
    // create only one blind messages map (key=msg_pvsk_total-1 value=prover_secret_key)
    let mut messages = BTreeMap::new();
    match gen_signature_message(&request.proverSecretKey) {
        Err(_) => return Err(JsValue::from_str("Failed to generate signature message")),
        Ok(m) => messages.insert(msg_pvsk_total - 1, m),
    };
    let nonce = ProofNonce::hash(&request.nonce);
    match Prover::new_blind_signature_context(&pk, &messages, &nonce) {
        Err(e) => Err(JsValue::from(&format!("{:?}", e))),
        Ok((cx, bf)) => {
            let response = BlindBlsSignatureRequestContextResponse {
                commitment: cx.commitment,
                proofOfHiddenMessages: cx.proof_of_hidden_messages,
                challengeHash: cx.challenge_hash,
                blindingFactor: bf,
            };
            Ok(serde_wasm_bindgen::to_value(&response).unwrap())
        }
    }
}

// inspired by bbs_verify_blind_signature_proof
#[wasm_bindgen(js_name = verifyBlindBlsSignatureRequest)]
pub async fn verify_blind_bls_signature_request(request: JsValue) -> Result<JsValue, JsValue> {
    set_panic_hook();
    let request: BlindBlsSignatureVerifyContextRequest = request.try_into()?;
    let msg_pvsk_total = request.messageCount + 1;
    let pk = request.publicKey.to_public_key(msg_pvsk_total)?;
    // let pk = request.pk;

    // blinded message (prover secret key) is always at the last message.
    let mut blinded = BTreeSet::new();
    blinded.insert(msg_pvsk_total - 1);

    let messages = (0..msg_pvsk_total)
        .filter(|i| !blinded.contains(i))
        .collect();

    let nonce = ProofNonce::hash(&request.nonce);
    let ctx = BlindSignatureContext {
        commitment: request.commitment,
        challenge_hash: request.challengeHash,
        proof_of_hidden_messages: request.proofOfHiddenMessages,
    };
    match ctx.verify(&messages, &pk, &nonce) {
        Err(e) => Ok(serde_wasm_bindgen::to_value(&BbsVerifyResponse {
            verified: false,
            error: Some(format!("{:?}", e)),
        })
        .unwrap()),
        Ok(b) => Ok(serde_wasm_bindgen::to_value(&BbsVerifyResponse {
            verified: b,
            error: None,
        })
        .unwrap()),
    }
}

// inspired by bbs_blind_sign
#[wasm_bindgen(js_name = blindBlsSign)]
pub async fn blind_bls_sign(request: JsValue) -> Result<JsValue, JsValue> {
    set_panic_hook();
    let request: BlindBlsSignContextRequest = request.try_into()?;
    let dpk_bytes = request.keyPair.publicKey.unwrap();

    let dpk = DeterministicPublicKey::from(array_ref![dpk_bytes, 0, G2_COMPRESSED_SIZE]);
    let pk_res = dpk.to_public_key(request.messages.len() + 1);
    let pk;
    match pk_res {
        Err(_) => return Err(JsValue::from_str("Failed to convert key")),
        Ok(p) => pk = p,
    };
    if request.keyPair.secretKey.is_none() {
        return Err(JsValue::from_str("Failed to sign"));
    }

    let mut messages: BTreeMap<usize, SignatureMessage> = BTreeMap::new();
    for (i, msg) in request.messages.iter().enumerate() {
        match gen_signature_message(msg) {
            Err(_) => return Err(JsValue::from_str("Failed to generate signature message")),
            Ok(m) => messages.insert(i, m),
        };
    }

    match BlindSignature::new(
        &request.commitment,
        &messages,
        &request.keyPair.secretKey.unwrap(),
        &pk,
    ) {
        Ok(s) => Ok(serde_wasm_bindgen::to_value(&s).unwrap()),
        Err(e) => Err(JsValue::from(&format!("{:?}", e))),
    }
}

// inspired by bbs_get_unblinded_signature
#[wasm_bindgen(js_name = unblindBlindBlsSignature)]
pub async fn unblind_blind_bls_signature(request: JsValue) -> Result<JsValue, JsValue> {
    set_panic_hook();
    let request: UnblindBlindSignatureRequest = request.try_into()?;
    Ok(
        serde_wasm_bindgen::to_value(&request.signature.to_unblinded(&request.blindingFactor))
            .unwrap(),
    )
}
