#![no_std]

use soroban_sdk::{contract, contractimpl, token, Address, Env, String};

mod escrow;
mod merchant;
mod payment;
mod storage;

pub use escrow::*;
pub use merchant::*;
pub use payment::*;
pub use storage::*;

#[cfg(test)]
mod test;

#[contract]
pub struct PaymentGateway;

fn calculate_fee(amount: i128, fee_bps: u32) -> i128 {
    let fee_bps = fee_bps as i128;
    (amount / 10_000) * fee_bps + ((amount % 10_000) * fee_bps) / 10_000
}

#[contractimpl]
impl PaymentGateway {
    /// Initialize the payment gateway contract
    ///
    /// # Arguments
    /// * `admin` - Admin address with full control
    /// * `platform_fee` - Platform fee in basis points (e.g., 250 = 2.5%)
    pub fn initialize(env: Env, admin: Address, platform_fee: u32) {
        if storage::has_admin(&env) {
            panic!("Contract already initialized");
        }

        if platform_fee > 10000 {
            panic!("Platform fee cannot exceed 100%");
        }

        admin.require_auth();

        storage::set_admin(&env, &admin);
        storage::set_platform_fee(&env, platform_fee);

        env.events()
            .publish(("initialized",), (admin, platform_fee));
    }

    /// Register a new merchant
    ///
    /// # Arguments
    /// * `merchant_id` - Unique merchant identifier
    /// * `merchant_address` - Merchant's Stellar address
    /// * `merchant_name` - Merchant display name
    pub fn register_merchant(
        env: Env,
        merchant_id: String,
        merchant_address: Address,
        merchant_name: String,
    ) {
        let admin = storage::get_admin(&env);
        admin.require_auth();

        if storage::has_merchant(&env, &merchant_id) {
            panic!("Merchant already registered");
        }

        let merchant = Merchant {
            id: merchant_id.clone(),
            address: merchant_address.clone(),
            name: merchant_name,
            active: true,
            total_volume: 0,
            transaction_count: 0,
        };

        storage::set_merchant(&env, &merchant_id, &merchant);

        env.events()
            .publish(("merchant_registered",), (merchant_id, merchant_address));
    }

    /// Process a payment
    ///
    /// # Arguments
    /// * `payer` - Customer making the payment
    /// * `merchant_id` - Merchant receiving the payment
    /// * `token` - Token contract address
    /// * `amount` - Payment amount
    /// * `order_id` - Merchant-scoped order identifier; reusing it for the same
    ///   merchant is rejected, while other merchants may use the same value
    pub fn process_payment(
        env: Env,
        payer: Address,
        merchant_id: String,
        token: Address,
        amount: i128,
        order_id: String,
    ) -> u64 {
        payer.require_auth();

        if amount <= 0 {
            panic!("Amount must be positive");
        }

        let merchant = storage::get_merchant(&env, &merchant_id)
            .unwrap_or_else(|| panic!("Merchant not found"));

        if !merchant.active {
            panic!("Merchant is not active");
        }

        // Order IDs are idempotent per merchant: reject a retry before any token
        // movement so the same order can never be settled twice.
        if storage::has_order_payment(&env, &merchant_id, &order_id) {
            panic!("Order already paid");
        }

        // Split the division first so valid large payments cannot overflow.
        let platform_fee = storage::get_platform_fee(&env);
        let fee_amount = calculate_fee(amount, platform_fee);
        let merchant_amount = amount - fee_amount;

        // Transfer payment
        let token_client = token::Client::new(&env, &token);

        token_client.transfer(&payer, &merchant.address, &merchant_amount);

        if fee_amount > 0 {
            let admin = storage::get_admin(&env);
            token_client.transfer(&payer, &admin, &fee_amount);
        }

        // Create payment record
        let payment_id = storage::get_next_payment_id(&env);
        let payment = Payment {
            id: payment_id,
            payer: payer.clone(),
            merchant_id: merchant_id.clone(),
            token: token.clone(),
            amount,
            fee_amount,
            order_id: order_id.clone(),
            timestamp: env.ledger().timestamp(),
            status: PaymentStatus::Completed,
        };

        storage::set_payment(&env, payment_id, &payment);
        // Reserve the order ID only once the transfers have succeeded. A panic
        // anywhere above rolls this entry back with the rest of the transaction.
        storage::set_order_payment(&env, &merchant_id, &order_id, payment_id);

        // Update merchant stats
        let mut updated_merchant = merchant;
        updated_merchant.total_volume = updated_merchant
            .total_volume
            .checked_add(amount)
            .unwrap_or_else(|| panic!("Merchant volume overflow"));
        updated_merchant.transaction_count += 1;
        storage::set_merchant(&env, &merchant_id, &updated_merchant);

        env.events().publish(
            ("payment_processed",),
            (payment_id, payer, merchant_id, amount),
        );

        payment_id
    }

    /// Create an escrow transaction
    ///
    /// # Arguments
    /// * `buyer` - Buyer address
    /// * `seller` - Seller address
    /// * `token` - Token contract address
    /// * `amount` - Escrow amount
    /// * `order_id` - Unique order identifier
    /// * `release_time` - Unix timestamp when funds can be auto-released
    pub fn create_escrow(
        env: Env,
        buyer: Address,
        seller: Address,
        token: Address,
        amount: i128,
        order_id: String,
        release_time: u64,
    ) -> u64 {
        buyer.require_auth();

        if amount <= 0 {
            panic!("Amount must be positive");
        }

        if release_time <= env.ledger().timestamp() {
            panic!("Release time must be in the future");
        }

        let escrow_id = storage::get_next_escrow_id(&env);
        let contract_address = env.current_contract_address();

        // Transfer funds to contract
        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&buyer, &contract_address, &amount);

        let escrow = Escrow {
            id: escrow_id,
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token.clone(),
            amount,
            order_id: order_id.clone(),
            created_at: env.ledger().timestamp(),
            release_time,
            status: EscrowStatus::Active,
        };

        storage::set_escrow(&env, escrow_id, &escrow);

        env.events()
            .publish(("escrow_created",), (escrow_id, buyer, seller, amount));

        escrow_id
    }

    /// Release escrow funds to seller
    ///
    /// # Arguments
    /// * `escrow_id` - Escrow transaction ID
    pub fn release_escrow(env: Env, escrow_id: u64) {
        let mut escrow =
            storage::get_escrow(&env, escrow_id).unwrap_or_else(|| panic!("Escrow not found"));

        if escrow.status != EscrowStatus::Active {
            panic!("Escrow is not active");
        }

        // Either buyer releases or auto-release after time
        let current_time = env.ledger().timestamp();
        let can_release = current_time >= escrow.release_time;

        if !can_release {
            escrow.buyer.require_auth();
        }

        // Transfer funds to seller
        let token_client = token::Client::new(&env, &escrow.token);
        let contract_address = env.current_contract_address();
        token_client.transfer(&contract_address, &escrow.seller, &escrow.amount);

        escrow.status = EscrowStatus::Released;
        storage::set_escrow(&env, escrow_id, &escrow);

        env.events().publish(
            ("escrow_released",),
            (escrow_id, escrow.seller, escrow.amount),
        );
    }

    /// Refund escrow to buyer
    ///
    /// # Arguments
    /// * `escrow_id` - Escrow transaction ID
    pub fn refund_escrow(env: Env, escrow_id: u64) {
        let mut escrow =
            storage::get_escrow(&env, escrow_id).unwrap_or_else(|| panic!("Escrow not found"));

        if escrow.status != EscrowStatus::Active {
            panic!("Escrow is not active");
        }

        // Only seller can initiate refund
        escrow.seller.require_auth();

        // Transfer funds back to buyer
        let token_client = token::Client::new(&env, &escrow.token);
        let contract_address = env.current_contract_address();
        token_client.transfer(&contract_address, &escrow.buyer, &escrow.amount);

        escrow.status = EscrowStatus::Refunded;
        storage::set_escrow(&env, escrow_id, &escrow);

        env.events().publish(
            ("escrow_refunded",),
            (escrow_id, escrow.buyer, escrow.amount),
        );
    }

    /// Get payment details
    pub fn get_payment(env: Env, payment_id: u64) -> Option<Payment> {
        storage::get_payment(&env, payment_id)
    }

    /// Get escrow details
    pub fn get_escrow(env: Env, escrow_id: u64) -> Option<Escrow> {
        storage::get_escrow(&env, escrow_id)
    }

    /// Get merchant details
    pub fn get_merchant(env: Env, merchant_id: String) -> Option<Merchant> {
        storage::get_merchant(&env, &merchant_id)
    }

    /// Update platform fee (admin only)
    pub fn update_platform_fee(env: Env, new_fee: u32) {
        let admin = storage::get_admin(&env);
        admin.require_auth();

        if new_fee > 10000 {
            panic!("Platform fee cannot exceed 100%");
        }

        storage::set_platform_fee(&env, new_fee);

        env.events().publish(("fee_updated",), new_fee);
    }

    /// Get current platform fee
    pub fn get_platform_fee(env: Env) -> u32 {
        storage::get_platform_fee(&env)
    }
}
