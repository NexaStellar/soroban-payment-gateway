use soroban_sdk::{contracttype, Address, Env, String};

use crate::{Escrow, Merchant, Payment};

// Ninety days at Stellar's nominal five-second ledger interval.
pub(crate) const STORAGE_TTL_LEDGERS: u32 = 90 * 17_280;

#[contracttype]
#[derive(Clone)]
pub(crate) enum DataKey {
    Admin,
    PlatformFee,
    PaymentCounter,
    EscrowCounter,
    Merchant(String),
    Payment(u64),
    Escrow(u64),
    /// Maps a merchant-scoped order ID to the payment that settled it.
    OrderPayment(String, String),
}

fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(STORAGE_TTL_LEDGERS, STORAGE_TTL_LEDGERS);
}

fn extend_persistent_ttl(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, STORAGE_TTL_LEDGERS, STORAGE_TTL_LEDGERS);
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
    extend_instance_ttl(env);
}

pub fn get_admin(env: &Env) -> Address {
    extend_instance_ttl(env);
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic!("Admin not set"))
}

pub fn has_admin(env: &Env) -> bool {
    extend_instance_ttl(env);
    env.storage().instance().has(&DataKey::Admin)
}

pub fn set_platform_fee(env: &Env, fee: u32) {
    env.storage().instance().set(&DataKey::PlatformFee, &fee);
    extend_instance_ttl(env);
}

pub fn get_platform_fee(env: &Env) -> u32 {
    extend_instance_ttl(env);
    env.storage()
        .instance()
        .get(&DataKey::PlatformFee)
        .unwrap_or(0)
}

pub fn set_merchant(env: &Env, merchant_id: &String, merchant: &Merchant) {
    let key = DataKey::Merchant(merchant_id.clone());
    env.storage().persistent().set(&key, merchant);
    extend_instance_ttl(env);
    extend_persistent_ttl(env, &key);
}

pub fn get_merchant(env: &Env, merchant_id: &String) -> Option<Merchant> {
    let key = DataKey::Merchant(merchant_id.clone());
    extend_instance_ttl(env);
    let merchant = env.storage().persistent().get(&key);
    if merchant.is_some() {
        extend_persistent_ttl(env, &key);
    }
    merchant
}

pub fn has_merchant(env: &Env, merchant_id: &String) -> bool {
    let key = DataKey::Merchant(merchant_id.clone());
    extend_instance_ttl(env);
    let exists = env.storage().persistent().has(&key);
    if exists {
        extend_persistent_ttl(env, &key);
    }
    exists
}

pub fn get_next_payment_id(env: &Env) -> u64 {
    extend_instance_ttl(env);
    let current: u64 = env
        .storage()
        .instance()
        .get(&DataKey::PaymentCounter)
        .unwrap_or(0);
    let next = current + 1;
    env.storage()
        .instance()
        .set(&DataKey::PaymentCounter, &next);
    next
}

pub fn set_payment(env: &Env, payment_id: u64, payment: &Payment) {
    let key = DataKey::Payment(payment_id);
    env.storage().persistent().set(&key, payment);
    extend_instance_ttl(env);
    extend_persistent_ttl(env, &key);
}

pub fn get_payment(env: &Env, payment_id: u64) -> Option<Payment> {
    let key = DataKey::Payment(payment_id);
    extend_instance_ttl(env);
    let payment = env.storage().persistent().get(&key);
    if payment.is_some() {
        extend_persistent_ttl(env, &key);
    }
    payment
}

pub fn set_order_payment(env: &Env, merchant_id: &String, order_id: &String, payment_id: u64) {
    let key = DataKey::OrderPayment(merchant_id.clone(), order_id.clone());
    env.storage().persistent().set(&key, &payment_id);
    extend_instance_ttl(env);
    extend_persistent_ttl(env, &key);
}

pub fn get_order_payment(env: &Env, merchant_id: &String, order_id: &String) -> Option<u64> {
    let key = DataKey::OrderPayment(merchant_id.clone(), order_id.clone());
    extend_instance_ttl(env);
    let payment_id = env.storage().persistent().get(&key);
    if payment_id.is_some() {
        extend_persistent_ttl(env, &key);
    }
    payment_id
}

pub fn has_order_payment(env: &Env, merchant_id: &String, order_id: &String) -> bool {
    let key = DataKey::OrderPayment(merchant_id.clone(), order_id.clone());
    extend_instance_ttl(env);
    let exists = env.storage().persistent().has(&key);
    if exists {
        extend_persistent_ttl(env, &key);
    }
    exists
}

pub fn get_next_escrow_id(env: &Env) -> u64 {
    extend_instance_ttl(env);
    let current: u64 = env
        .storage()
        .instance()
        .get(&DataKey::EscrowCounter)
        .unwrap_or(0);
    let next = current + 1;
    env.storage().instance().set(&DataKey::EscrowCounter, &next);
    next
}

pub fn set_escrow(env: &Env, escrow_id: u64, escrow: &Escrow) {
    let key = DataKey::Escrow(escrow_id);
    env.storage().persistent().set(&key, escrow);
    extend_instance_ttl(env);
    extend_persistent_ttl(env, &key);
}

pub fn get_escrow(env: &Env, escrow_id: u64) -> Option<Escrow> {
    let key = DataKey::Escrow(escrow_id);
    extend_instance_ttl(env);
    let escrow = env.storage().persistent().get(&key);
    if escrow.is_some() {
        extend_persistent_ttl(env, &key);
    }
    escrow
}
