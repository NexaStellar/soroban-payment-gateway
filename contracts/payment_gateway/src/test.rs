use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke},
    token, Address, Env, IntoVal, String,
};

fn setup() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_id = env.register(PaymentGateway, ());
    PaymentGatewayClient::new(&env, &contract_id).initialize(&admin, &250);

    (env, contract_id, admin, token_address)
}

fn register_merchant(
    client: &PaymentGatewayClient<'_>,
    env: &Env,
    merchant_id: &str,
    merchant_address: &Address,
) {
    client.register_merchant(
        &String::from_str(env, merchant_id),
        merchant_address,
        &String::from_str(env, "Test merchant"),
    );
}

#[test]
fn initializes_with_configured_platform_fee() {
    let (env, contract_id, _, _) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);

    assert_eq!(client.get_platform_fee(), 250);
}

#[test]
#[should_panic(expected = "Platform fee cannot exceed 100%")]
fn rejects_platform_fee_above_100_percent() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(PaymentGateway, ());
    let client = PaymentGatewayClient::new(&env, &contract_id);

    client.initialize(&admin, &10_001);
}

#[test]
fn payment_splits_amount_and_records_payment_and_merchant_volume() {
    let (env, contract_id, admin, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let payer = Address::generate(&env);
    let merchant_address = Address::generate(&env);
    let merchant_id = String::from_str(&env, "merchant-1");
    let order_id = String::from_str(&env, "order-1");

    token::StellarAssetClient::new(&env, &token_address).mint(&payer, &10_000);
    register_merchant(&client, &env, "merchant-1", &merchant_address);

    let payment_id =
        client.process_payment(&payer, &merchant_id, &token_address, &10_000, &order_id);

    let token = token::Client::new(&env, &token_address);
    assert_eq!(token.balance(&payer), 0);
    assert_eq!(token.balance(&merchant_address), 9_750);
    assert_eq!(token.balance(&admin), 250);

    let payment = client.get_payment(&payment_id).unwrap();
    assert_eq!(payment.amount, 10_000);
    assert_eq!(payment.fee_amount, 250);
    assert_eq!(payment.order_id, order_id);
    assert_eq!(payment.status, PaymentStatus::Completed);

    let merchant = client.get_merchant(&merchant_id).unwrap();
    assert_eq!(merchant.total_volume, 10_000);
    assert_eq!(merchant.transaction_count, 1);
}

#[test]
fn buyer_can_release_escrow_before_release_time() {
    let (env, contract_id, _, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    env.ledger().set_timestamp(100);
    token::StellarAssetClient::new(&env, &token_address).mint(&buyer, &500);
    let escrow_id = client.create_escrow(
        &buyer,
        &seller,
        &token_address,
        &500,
        &String::from_str(&env, "order-escrow"),
        &200,
    );

    assert_eq!(
        token::Client::new(&env, &token_address).balance(&contract_id),
        500
    );

    client.release_escrow(&escrow_id);

    assert_eq!(
        token::Client::new(&env, &token_address).balance(&seller),
        500
    );
    assert_eq!(
        client.get_escrow(&escrow_id).unwrap().status,
        EscrowStatus::Released
    );
}

#[test]
fn anyone_can_release_escrow_after_release_time() {
    let (env, contract_id, _, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    env.ledger().set_timestamp(100);
    token::StellarAssetClient::new(&env, &token_address).mint(&buyer, &500);
    let escrow_id = client.create_escrow(
        &buyer,
        &seller,
        &token_address,
        &500,
        &String::from_str(&env, "order-auto-release"),
        &200,
    );

    env.ledger().set_timestamp(200);
    client.release_escrow(&escrow_id);

    assert_eq!(
        token::Client::new(&env, &token_address).balance(&seller),
        500
    );
    assert_eq!(
        client.get_escrow(&escrow_id).unwrap().status,
        EscrowStatus::Released
    );
}

#[test]
fn seller_can_refund_active_escrow_to_buyer() {
    let (env, contract_id, _, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    env.ledger().set_timestamp(100);
    token::StellarAssetClient::new(&env, &token_address).mint(&buyer, &500);
    let escrow_id = client.create_escrow(
        &buyer,
        &seller,
        &token_address,
        &500,
        &String::from_str(&env, "order-refund"),
        &200,
    );

    client.refund_escrow(&escrow_id);

    assert_eq!(
        token::Client::new(&env, &token_address).balance(&buyer),
        500
    );
    assert_eq!(
        client.get_escrow(&escrow_id).unwrap().status,
        EscrowStatus::Refunded
    );
}

#[test]
fn fee_calculation_handles_maximum_i128_amount_without_overflow() {
    assert_eq!(calculate_fee(i128::MAX, 10_000), i128::MAX);
    assert_eq!(calculate_fee(i128::MAX, 0), 0);
    assert_eq!(calculate_fee(399, 250), 9);
    assert_eq!(calculate_fee(400, 250), 10);
}

// =========================================================================
// Authorization Boundaries: Admin Functions
// =========================================================================

#[test]
fn non_admin_cannot_initialize() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let contract_id = env.register(PaymentGateway, ());
    let client = PaymentGatewayClient::new(&env, &contract_id);

    // Call without admin authorization fails
    let res = client
        .mock_auths(&[MockAuth {
            address: &non_admin,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "initialize",
                args: (&admin, 250u32).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_initialize(&admin, &250);
    assert!(res.is_err());

    // Also unauthenticated call fails
    let unauth_res = client.set_auths(&[]).try_initialize(&admin, &250);
    assert!(unauth_res.is_err());

    // Contract was not initialized
    assert_eq!(client.get_platform_fee(), 0);

    // Initializing with admin authorization succeeds
    let init_res = client
        .mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "initialize",
                args: (&admin, 250u32).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_initialize(&admin, &250);
    assert!(init_res.is_ok());
    assert_eq!(client.get_platform_fee(), 250);
}

#[test]
fn cannot_initialize_already_initialized_contract() {
    let (env, contract_id, admin, _) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let other = Address::generate(&env);

    let res = client.mock_all_auths().try_initialize(&other, &500);
    assert!(res.is_err());
    assert_eq!(client.get_platform_fee(), 250);

    let res_admin = client.mock_all_auths().try_initialize(&admin, &500);
    assert!(res_admin.is_err());
    assert_eq!(client.get_platform_fee(), 250);
}

#[test]
fn non_admin_cannot_register_merchant() {
    let (env, contract_id, _, _) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let non_admin = Address::generate(&env);
    let merchant_address = Address::generate(&env);
    let merchant_id = String::from_str(&env, "merchant-1");
    let merchant_name = String::from_str(&env, "Merchant One");

    // Call with non-admin auth fails
    let res = client
        .mock_auths(&[MockAuth {
            address: &non_admin,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "register_merchant",
                args: (
                    merchant_id.clone(),
                    merchant_address.clone(),
                    merchant_name.clone(),
                )
                    .into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_register_merchant(&merchant_id, &merchant_address, &merchant_name);
    assert!(res.is_err());

    // Unauthenticated call also fails
    let unauth_res = client.set_auths(&[]).try_register_merchant(
        &merchant_id,
        &merchant_address,
        &merchant_name,
    );
    assert!(unauth_res.is_err());

    // Merchant was not registered in storage
    assert!(client.get_merchant(&merchant_id).is_none());
}

#[test]
fn non_admin_cannot_update_platform_fee() {
    let (env, contract_id, _, _) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let non_admin = Address::generate(&env);

    // Call with non-admin auth fails
    let res = client
        .mock_auths(&[MockAuth {
            address: &non_admin,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "update_platform_fee",
                args: (500u32,).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_update_platform_fee(&500);
    assert!(res.is_err());

    // Unauthenticated call fails
    let unauth_res = client.set_auths(&[]).try_update_platform_fee(&500);
    assert!(unauth_res.is_err());

    // Fee remains unchanged
    assert_eq!(client.get_platform_fee(), 250);
}

#[test]
fn admin_can_update_platform_fee_and_rejects_fee_above_100_percent() {
    let (env, contract_id, admin, _) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);

    // Admin updates fee to 500 bps (5%)
    let res = client
        .mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "update_platform_fee",
                args: (500u32,).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_update_platform_fee(&500);
    assert!(res.is_ok());
    assert_eq!(client.get_platform_fee(), 500);

    // Updating fee above 10,000 (100%) fails and preserves previous fee
    let overflow_res = client
        .mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "update_platform_fee",
                args: (10_001u32,).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_update_platform_fee(&10_001);
    assert!(overflow_res.is_err());
    assert_eq!(client.get_platform_fee(), 500);
}

// =========================================================================
// Authorization Boundaries: Payer and Escrow Parties
// =========================================================================

#[test]
fn caller_without_payer_auth_cannot_process_payment() {
    let (env, contract_id, admin, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let payer = Address::generate(&env);
    let attacker = Address::generate(&env);
    let merchant_address = Address::generate(&env);
    let merchant_id = String::from_str(&env, "merchant-1");
    let order_id = String::from_str(&env, "order-auth-1");

    token::StellarAssetClient::new(&env, &token_address).mint(&payer, &10_000);
    register_merchant(&client, &env, "merchant-1", &merchant_address);

    // Attacker authorizes the call instead of payer
    let res = client
        .mock_auths(&[MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "process_payment",
                args: (
                    payer.clone(),
                    merchant_id.clone(),
                    token_address.clone(),
                    10_000i128,
                    order_id.clone(),
                )
                    .into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_process_payment(&payer, &merchant_id, &token_address, &10_000, &order_id);
    assert!(res.is_err());

    // Unauthenticated call also fails
    let unauth_res = client.set_auths(&[]).try_process_payment(
        &payer,
        &merchant_id,
        &token_address,
        &10_000,
        &order_id,
    );
    assert!(unauth_res.is_err());

    // Post-call state and balances are completely unchanged
    let token = token::Client::new(&env, &token_address);
    assert_eq!(token.balance(&payer), 10_000);
    assert_eq!(token.balance(&merchant_address), 0);
    assert_eq!(token.balance(&admin), 0);
    assert!(client.get_payment(&1).is_none());

    let merchant = client.get_merchant(&merchant_id).unwrap();
    assert_eq!(merchant.total_volume, 0);
    assert_eq!(merchant.transaction_count, 0);
}

#[test]
fn caller_without_buyer_auth_cannot_create_escrow() {
    let (env, contract_id, _, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let attacker = Address::generate(&env);
    let order_id = String::from_str(&env, "order-escrow-auth");

    env.ledger().set_timestamp(100);
    token::StellarAssetClient::new(&env, &token_address).mint(&buyer, &1_000);

    // Attacker authorizes instead of buyer
    let res = client
        .mock_auths(&[MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "create_escrow",
                args: (
                    buyer.clone(),
                    seller.clone(),
                    token_address.clone(),
                    500i128,
                    order_id.clone(),
                    200u64,
                )
                    .into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_create_escrow(&buyer, &seller, &token_address, &500, &order_id, &200);
    assert!(res.is_err());

    // Unauthenticated call also fails
    let unauth_res = client.set_auths(&[]).try_create_escrow(
        &buyer,
        &seller,
        &token_address,
        &500,
        &order_id,
        &200,
    );
    assert!(unauth_res.is_err());

    // Post-call state and balances are unchanged
    let token = token::Client::new(&env, &token_address);
    assert_eq!(token.balance(&buyer), 1_000);
    assert_eq!(token.balance(&contract_id), 0);
    assert!(client.get_escrow(&1).is_none());
}

#[test]
fn only_buyer_may_release_escrow_before_deadline() {
    let (env, contract_id, _, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let stranger = Address::generate(&env);

    env.ledger().set_timestamp(100);
    token::StellarAssetClient::new(&env, &token_address).mint(&buyer, &500);
    let escrow_id = client.mock_all_auths().create_escrow(
        &buyer,
        &seller,
        &token_address,
        &500,
        &String::from_str(&env, "order-early-release"),
        &200,
    );

    let token = token::Client::new(&env, &token_address);
    assert_eq!(token.balance(&contract_id), 500);

    // Current time is before release_time (100 < 200).
    // Seller attempting release before deadline fails
    let seller_res = client
        .mock_auths(&[MockAuth {
            address: &seller,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "release_escrow",
                args: (escrow_id,).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_release_escrow(&escrow_id);
    assert!(seller_res.is_err());

    // Stranger attempting release before deadline fails
    let stranger_res = client
        .mock_auths(&[MockAuth {
            address: &stranger,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "release_escrow",
                args: (escrow_id,).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_release_escrow(&escrow_id);
    assert!(stranger_res.is_err());

    // Unauthenticated release before deadline fails
    let unauth_res = client.set_auths(&[]).try_release_escrow(&escrow_id);
    assert!(unauth_res.is_err());

    // State and balances remain unchanged
    assert_eq!(token.balance(&contract_id), 500);
    assert_eq!(token.balance(&seller), 0);
    assert_eq!(
        client.get_escrow(&escrow_id).unwrap().status,
        EscrowStatus::Active
    );

    // Buyer releases before deadline -> succeeds
    let buyer_res = client.mock_all_auths().try_release_escrow(&escrow_id);
    assert!(buyer_res.is_ok());

    assert_eq!(token.balance(&contract_id), 0);
    assert_eq!(token.balance(&seller), 500);
    assert_eq!(
        client.get_escrow(&escrow_id).unwrap().status,
        EscrowStatus::Released
    );
}

#[test]
fn only_seller_may_request_seller_initiated_refund() {
    let (env, contract_id, _, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let stranger = Address::generate(&env);

    env.ledger().set_timestamp(100);
    token::StellarAssetClient::new(&env, &token_address).mint(&buyer, &500);
    let escrow_id = client.mock_all_auths().create_escrow(
        &buyer,
        &seller,
        &token_address,
        &500,
        &String::from_str(&env, "order-refund-auth"),
        &200,
    );

    let token = token::Client::new(&env, &token_address);
    assert_eq!(token.balance(&contract_id), 500);
    assert_eq!(token.balance(&buyer), 0);

    // Buyer attempting refund fails
    let buyer_res = client
        .mock_auths(&[MockAuth {
            address: &buyer,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "refund_escrow",
                args: (escrow_id,).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_refund_escrow(&escrow_id);
    assert!(buyer_res.is_err());

    // Stranger attempting refund fails
    let stranger_res = client
        .mock_auths(&[MockAuth {
            address: &stranger,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "refund_escrow",
                args: (escrow_id,).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_refund_escrow(&escrow_id);
    assert!(stranger_res.is_err());

    // Unauthenticated refund fails
    let unauth_res = client.set_auths(&[]).try_refund_escrow(&escrow_id);
    assert!(unauth_res.is_err());

    // State and balances remain unchanged
    assert_eq!(token.balance(&contract_id), 500);
    assert_eq!(token.balance(&buyer), 0);
    assert_eq!(
        client.get_escrow(&escrow_id).unwrap().status,
        EscrowStatus::Active
    );

    // Seller initiates refund -> succeeds
    let seller_res = client.mock_all_auths().try_refund_escrow(&escrow_id);
    assert!(seller_res.is_ok());

    assert_eq!(token.balance(&contract_id), 0);
    assert_eq!(token.balance(&buyer), 500);
    assert_eq!(
        client.get_escrow(&escrow_id).unwrap().status,
        EscrowStatus::Refunded
    );
}

// =========================================================================
// Input Validation: Zero/Negative Amounts, Invalid Times, Unknown IDs
// =========================================================================

#[test]
fn payment_rejects_zero_and_negative_amounts() {
    let (env, contract_id, admin, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let payer = Address::generate(&env);
    let merchant_address = Address::generate(&env);
    let merchant_id = String::from_str(&env, "merchant-1");
    let order_id = String::from_str(&env, "order-invalid-amount");

    token::StellarAssetClient::new(&env, &token_address).mint(&payer, &5_000);
    register_merchant(&client, &env, "merchant-1", &merchant_address);

    let token = token::Client::new(&env, &token_address);

    // Zero amount fails
    let zero_res = client.mock_all_auths().try_process_payment(
        &payer,
        &merchant_id,
        &token_address,
        &0,
        &order_id,
    );
    assert!(zero_res.is_err());

    // Negative amount fails
    let neg_res = client.mock_all_auths().try_process_payment(
        &payer,
        &merchant_id,
        &token_address,
        &-100,
        &order_id,
    );
    assert!(neg_res.is_err());

    // Post-call balances and merchant records unchanged
    assert_eq!(token.balance(&payer), 5_000);
    assert_eq!(token.balance(&merchant_address), 0);
    assert_eq!(token.balance(&admin), 0);
    assert!(client.get_payment(&1).is_none());

    let merchant = client.get_merchant(&merchant_id).unwrap();
    assert_eq!(merchant.total_volume, 0);
    assert_eq!(merchant.transaction_count, 0);
}

#[test]
fn payment_fails_for_unknown_or_duplicate_merchant() {
    let (env, contract_id, admin, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let payer = Address::generate(&env);
    let unknown_merchant_id = String::from_str(&env, "unknown-merchant");
    let order_id = String::from_str(&env, "order-unknown-merchant");

    token::StellarAssetClient::new(&env, &token_address).mint(&payer, &5_000);

    // Payment to non-existent merchant fails
    let res = client.mock_all_auths().try_process_payment(
        &payer,
        &unknown_merchant_id,
        &token_address,
        &1_000,
        &order_id,
    );
    assert!(res.is_err());

    // Balances and storage remain untouched
    let token = token::Client::new(&env, &token_address);
    assert_eq!(token.balance(&payer), 5_000);
    assert_eq!(token.balance(&admin), 0);
    assert!(client.get_payment(&1).is_none());

    // Duplicate merchant registration fails
    let merchant_address = Address::generate(&env);
    register_merchant(&client, &env, "merchant-1", &merchant_address);

    let dup_res = client.mock_all_auths().try_register_merchant(
        &String::from_str(&env, "merchant-1"),
        &merchant_address,
        &String::from_str(&env, "Duplicate Merchant"),
    );
    assert!(dup_res.is_err());
}

#[test]
fn escrow_rejects_zero_and_negative_amounts() {
    let (env, contract_id, _, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let order_id = String::from_str(&env, "order-escrow-amount");

    env.ledger().set_timestamp(100);
    token::StellarAssetClient::new(&env, &token_address).mint(&buyer, &2_000);

    let token = token::Client::new(&env, &token_address);

    // Zero amount fails
    let zero_res = client.mock_all_auths().try_create_escrow(
        &buyer,
        &seller,
        &token_address,
        &0,
        &order_id,
        &200,
    );
    assert!(zero_res.is_err());

    // Negative amount fails
    let neg_res = client.mock_all_auths().try_create_escrow(
        &buyer,
        &seller,
        &token_address,
        &-500,
        &order_id,
        &200,
    );
    assert!(neg_res.is_err());

    // Balances and records unchanged
    assert_eq!(token.balance(&buyer), 2_000);
    assert_eq!(token.balance(&contract_id), 0);
    assert!(client.get_escrow(&1).is_none());
}

#[test]
fn escrow_rejects_past_or_current_release_time() {
    let (env, contract_id, _, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let order_id = String::from_str(&env, "order-invalid-time");

    env.ledger().set_timestamp(500);
    token::StellarAssetClient::new(&env, &token_address).mint(&buyer, &2_000);

    let token = token::Client::new(&env, &token_address);

    // Release time equal to current timestamp fails
    let equal_res = client.mock_all_auths().try_create_escrow(
        &buyer,
        &seller,
        &token_address,
        &500,
        &order_id,
        &500,
    );
    assert!(equal_res.is_err());

    // Release time in the past fails
    let past_res = client.mock_all_auths().try_create_escrow(
        &buyer,
        &seller,
        &token_address,
        &500,
        &order_id,
        &499,
    );
    assert!(past_res.is_err());

    // Balances and storage remain untouched
    assert_eq!(token.balance(&buyer), 2_000);
    assert_eq!(token.balance(&contract_id), 0);
    assert!(client.get_escrow(&1).is_none());
}

#[test]
fn release_and_refund_fail_for_unknown_escrow_id() {
    let (env, contract_id, _, _) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);

    let rel_res = client.mock_all_auths().try_release_escrow(&999);
    assert!(rel_res.is_err());

    let ref_res = client.mock_all_auths().try_refund_escrow(&999);
    assert!(ref_res.is_err());
}

// =========================================================================
// Lifecycle Finalization: Cannot Release or Refund Finalized Escrow
// =========================================================================

#[test]
fn releasing_or_refunding_already_released_escrow_fails() {
    let (env, contract_id, _, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    env.ledger().set_timestamp(100);
    token::StellarAssetClient::new(&env, &token_address).mint(&buyer, &500);
    let escrow_id = client.mock_all_auths().create_escrow(
        &buyer,
        &seller,
        &token_address,
        &500,
        &String::from_str(&env, "order-released-repeat"),
        &200,
    );

    // Buyer releases escrow
    client.mock_all_auths().release_escrow(&escrow_id);

    let token = token::Client::new(&env, &token_address);
    assert_eq!(token.balance(&contract_id), 0);
    assert_eq!(token.balance(&seller), 500);
    assert_eq!(token.balance(&buyer), 0);
    assert_eq!(
        client.get_escrow(&escrow_id).unwrap().status,
        EscrowStatus::Released
    );

    // Re-releasing released escrow fails
    let re_release_res = client.mock_all_auths().try_release_escrow(&escrow_id);
    assert!(re_release_res.is_err());

    // Refunding released escrow fails
    let refund_res = client.mock_all_auths().try_refund_escrow(&escrow_id);
    assert!(refund_res.is_err());

    // State and balances remain unchanged
    assert_eq!(token.balance(&contract_id), 0);
    assert_eq!(token.balance(&seller), 500);
    assert_eq!(token.balance(&buyer), 0);
    assert_eq!(
        client.get_escrow(&escrow_id).unwrap().status,
        EscrowStatus::Released
    );
}

#[test]
fn releasing_or_refunding_already_refunded_escrow_fails() {
    let (env, contract_id, _, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    env.ledger().set_timestamp(100);
    token::StellarAssetClient::new(&env, &token_address).mint(&buyer, &500);
    let escrow_id = client.mock_all_auths().create_escrow(
        &buyer,
        &seller,
        &token_address,
        &500,
        &String::from_str(&env, "order-refunded-repeat"),
        &200,
    );

    // Seller refunds escrow
    client.mock_all_auths().refund_escrow(&escrow_id);

    let token = token::Client::new(&env, &token_address);
    assert_eq!(token.balance(&contract_id), 0);
    assert_eq!(token.balance(&buyer), 500);
    assert_eq!(token.balance(&seller), 0);
    assert_eq!(
        client.get_escrow(&escrow_id).unwrap().status,
        EscrowStatus::Refunded
    );

    // Re-refunding refunded escrow fails
    let re_refund_res = client.mock_all_auths().try_refund_escrow(&escrow_id);
    assert!(re_refund_res.is_err());

    // Releasing refunded escrow fails
    let release_res = client.mock_all_auths().try_release_escrow(&escrow_id);
    assert!(release_res.is_err());

    // State and balances remain unchanged
    assert_eq!(token.balance(&contract_id), 0);
    assert_eq!(token.balance(&buyer), 500);
    assert_eq!(token.balance(&seller), 0);
    assert_eq!(
        client.get_escrow(&escrow_id).unwrap().status,
        EscrowStatus::Refunded
    );
}

// =========================================================================
// Atomicity: Failed Token Operations Leave Balances and Records Unchanged
// =========================================================================

#[test]
fn failed_payment_due_to_insufficient_balance_leaves_state_unchanged() {
    let (env, contract_id, admin, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let payer = Address::generate(&env);
    let merchant_address = Address::generate(&env);
    let merchant_id = String::from_str(&env, "merchant-1");
    let order_id = String::from_str(&env, "order-insufficient-funds");

    // Mint only 300 tokens to payer, but attempt payment of 1,000
    token::StellarAssetClient::new(&env, &token_address).mint(&payer, &300);
    register_merchant(&client, &env, "merchant-1", &merchant_address);

    let res = client.mock_all_auths().try_process_payment(
        &payer,
        &merchant_id,
        &token_address,
        &1_000,
        &order_id,
    );
    assert!(res.is_err());

    // Atomicity check: balances and stored records must remain completely unchanged
    let token = token::Client::new(&env, &token_address);
    assert_eq!(token.balance(&payer), 300);
    assert_eq!(token.balance(&merchant_address), 0);
    assert_eq!(token.balance(&admin), 0);
    assert!(client.get_payment(&1).is_none());

    let merchant = client.get_merchant(&merchant_id).unwrap();
    assert_eq!(merchant.total_volume, 0);
    assert_eq!(merchant.transaction_count, 0);
}

// =========================================================================
// Idempotent Order IDs: Duplicates, Cross-Merchant Reuse, Failed Transfers
// =========================================================================

#[test]
fn duplicate_payment_for_same_merchant_order_is_rejected() {
    let (env, contract_id, admin, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let payer = Address::generate(&env);
    let merchant_address = Address::generate(&env);
    let merchant_id = String::from_str(&env, "merchant-1");
    let order_id = String::from_str(&env, "order-duplicate");

    token::StellarAssetClient::new(&env, &token_address).mint(&payer, &20_000);
    register_merchant(&client, &env, "merchant-1", &merchant_address);

    // First payment for the merchant/order pair succeeds and records its ID
    let payment_id =
        client.process_payment(&payer, &merchant_id, &token_address, &10_000, &order_id);
    assert_eq!(payment_id, 1);
    assert_eq!(client.get_payment(&payment_id).unwrap().order_id, order_id);

    // A retry for the same order fails even though the payer could afford it
    let res = client.mock_all_auths().try_process_payment(
        &payer,
        &merchant_id,
        &token_address,
        &10_000,
        &order_id,
    );
    assert!(res.is_err());

    // No second charge, no second payment record, no stats change
    let token = token::Client::new(&env, &token_address);
    assert_eq!(token.balance(&payer), 10_000);
    assert_eq!(token.balance(&merchant_address), 9_750);
    assert_eq!(token.balance(&admin), 250);
    assert!(client.get_payment(&2).is_none());

    let merchant = client.get_merchant(&merchant_id).unwrap();
    assert_eq!(merchant.total_volume, 10_000);
    assert_eq!(merchant.transaction_count, 1);
}

#[test]
#[should_panic(expected = "Order already paid")]
fn duplicate_order_is_rejected_before_any_token_transfer() {
    let (env, contract_id, _, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let payer = Address::generate(&env);
    let broke_payer = Address::generate(&env);
    let merchant_address = Address::generate(&env);
    let merchant_id = String::from_str(&env, "merchant-1");
    let order_id = String::from_str(&env, "order-check-order-of-operations");

    token::StellarAssetClient::new(&env, &token_address).mint(&payer, &10_000);
    register_merchant(&client, &env, "merchant-1", &merchant_address);

    client.process_payment(&payer, &merchant_id, &token_address, &10_000, &order_id);

    // The retry pays with an unfunded account, so an insufficient-balance
    // failure would mean the transfer was attempted before the duplicate check.
    client.process_payment(
        &broke_payer,
        &merchant_id,
        &token_address,
        &10_000,
        &order_id,
    );
}

#[test]
fn same_order_id_can_be_paid_to_different_merchants() {
    let (env, contract_id, _, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let payer = Address::generate(&env);
    let first_merchant_address = Address::generate(&env);
    let second_merchant_address = Address::generate(&env);
    let first_merchant_id = String::from_str(&env, "merchant-1");
    let second_merchant_id = String::from_str(&env, "merchant-2");
    let order_id = String::from_str(&env, "shared-order-id");

    token::StellarAssetClient::new(&env, &token_address).mint(&payer, &20_000);
    register_merchant(&client, &env, "merchant-1", &first_merchant_address);
    register_merchant(&client, &env, "merchant-2", &second_merchant_address);

    let first_payment_id = client.process_payment(
        &payer,
        &first_merchant_id,
        &token_address,
        &10_000,
        &order_id,
    );
    let second_payment_id = client.process_payment(
        &payer,
        &second_merchant_id,
        &token_address,
        &10_000,
        &order_id,
    );

    assert_eq!(first_payment_id, 1);
    assert_eq!(second_payment_id, 2);

    let token = token::Client::new(&env, &token_address);
    assert_eq!(token.balance(&payer), 0);
    assert_eq!(token.balance(&first_merchant_address), 9_750);
    assert_eq!(token.balance(&second_merchant_address), 9_750);

    assert_eq!(
        client.get_payment(&first_payment_id).unwrap().merchant_id,
        first_merchant_id
    );
    assert_eq!(
        client.get_payment(&second_payment_id).unwrap().merchant_id,
        second_merchant_id
    );

    assert_eq!(
        client
            .get_merchant(&first_merchant_id)
            .unwrap()
            .transaction_count,
        1
    );
    assert_eq!(
        client
            .get_merchant(&second_merchant_id)
            .unwrap()
            .transaction_count,
        1
    );
}

#[test]
fn failed_token_transfer_does_not_reserve_order_id() {
    let (env, contract_id, admin, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let payer = Address::generate(&env);
    let merchant_address = Address::generate(&env);
    let merchant_id = String::from_str(&env, "merchant-1");
    let order_id = String::from_str(&env, "order-retry-after-failure");

    token::StellarAssetClient::new(&env, &token_address).mint(&payer, &300);
    register_merchant(&client, &env, "merchant-1", &merchant_address);

    // Insufficient balance aborts the payment
    let res = client.mock_all_auths().try_process_payment(
        &payer,
        &merchant_id,
        &token_address,
        &1_000,
        &order_id,
    );
    assert!(res.is_err());
    assert!(client.get_payment(&1).is_none());

    // Once funded, the same order ID can still be paid
    token::StellarAssetClient::new(&env, &token_address).mint(&payer, &700);
    let payment_id =
        client.process_payment(&payer, &merchant_id, &token_address, &1_000, &order_id);

    assert_eq!(payment_id, 1);

    let token = token::Client::new(&env, &token_address);
    assert_eq!(token.balance(&payer), 0);
    assert_eq!(token.balance(&merchant_address), 975);
    assert_eq!(token.balance(&admin), 25);

    let merchant = client.get_merchant(&merchant_id).unwrap();
    assert_eq!(merchant.total_volume, 1_000);
    assert_eq!(merchant.transaction_count, 1);
}

#[test]
fn failed_escrow_creation_due_to_insufficient_balance_leaves_state_unchanged() {
    let (env, contract_id, _, token_address) = setup();
    let client = PaymentGatewayClient::new(&env, &contract_id);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let order_id = String::from_str(&env, "order-escrow-insufficient");

    env.ledger().set_timestamp(100);
    // Mint only 200 tokens to buyer, but attempt escrow of 1,000
    token::StellarAssetClient::new(&env, &token_address).mint(&buyer, &200);

    let res = client.mock_all_auths().try_create_escrow(
        &buyer,
        &seller,
        &token_address,
        &1_000,
        &order_id,
        &200,
    );
    assert!(res.is_err());

    // Atomicity check: balances and stored records must remain completely unchanged
    let token = token::Client::new(&env, &token_address);
    assert_eq!(token.balance(&buyer), 200);
    assert_eq!(token.balance(&contract_id), 0);
    assert!(client.get_escrow(&1).is_none());
}
