# Soroban Payment Gateway

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Stellar](https://img.shields.io/badge/Stellar-Soroban-blue)](https://stellar.org)
[![CI](https://github.com/NexaStellar/soroban-payment-gateway/actions/workflows/ci.yml/badge.svg)](https://github.com/NexaStellar/soroban-payment-gateway/actions/workflows/ci.yml)

A small Soroban smart contract prototype for token payments, platform-fee distribution, merchant records, and buyer-funded escrow. It is an unaudited alpha and is not suitable for production funds.

## 📚 Documentation

The [Soroban Payment Gateway documentation site](https://nexastellar.github.io/soroban-payment-gateway/) covers the contract interface, events, security limitations, and local development workflow.

## 🌟 Features

- **Token payments**: Transfer Soroban token-contract balances directly from a payer to a registered merchant and the admin fee recipient
- **Idempotent order IDs**: A merchant order can only be paid once, so client retries cannot double-charge a payer
- **Escrow**: Hold tokens in the contract until the buyer releases early, anyone releases after the deadline, or the seller refunds the buyer
- **Merchant registry**: Admin registration and per-merchant volume/count tracking
- **Platform fee**: Admin-configurable fee in basis points, capped at 10,000 (100%)
- **Events and records**: Emit lifecycle events and store payment and escrow details

The contract currently has no dispute resolution, subscriptions, merchant update/deactivation API, or on-chain refund-request flow.

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────┐
│           Payment Gateway Contract              │
├─────────────────────────────────────────────────┤
│  • Payment Processing                           │
│  • Escrow Management                            │
│  • Merchant Registry                            │
│  • Fee Distribution                             │
│  • Refund Logic                                 │
└─────────────────────────────────────────────────┘
         │                    │
         ▼                    ▼
┌─────────────────┐  ┌─────────────────┐
│  Stellar Assets │  │   Event System  │
│  (XLM, Tokens)  │  │  (Payment logs) │
└─────────────────┘  └─────────────────┘
```

## Status and limitations

- **Status:** Alpha prototype; APIs and storage may change.
- **Network:** Tests use Soroban's local test environment. No deployment or production-readiness claim is made.
- **Security:** Not audited. Do not use with real funds.
- **Assets:** The contract calls the Soroban token interface. It does not support arbitrary Stellar assets unless they are exposed through a compatible token contract.
- **Trust:** The admin receives platform fees and can set the fee up to 100%. Escrow has no dispute process; before the deadline the buyer can release and the seller can refund.

## 📋 Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (latest stable)
- `wasm32-unknown-unknown` target (only for Wasm builds)

## 🚀 Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/NexaStellar/soroban-payment-gateway.git
cd soroban-payment-gateway

# Build the contract
make build

# Run tests
make test
```

## Contract interface

| Method | Behavior |
|---|---|
| `initialize(admin, platform_fee)` | Set the admin and initial fee once |
| `register_merchant(id, address, name)` | Add a merchant; admin authorization required |
| `process_payment(payer, merchant_id, token, amount, order_id)` | Transfer payment and fee directly from payer; payer authorization required; a second payment for the same `(merchant_id, order_id)` is rejected |
| `create_escrow(buyer, seller, token, amount, order_id, release_time)` | Transfer tokens into contract custody; buyer authorization required |
| `release_escrow(id)` | Buyer may release before the deadline; after it, anyone may trigger release |
| `refund_escrow(id)` | Seller may refund an active escrow to its buyer |
| `get_payment(id)`, `get_escrow(id)`, `get_merchant(id)` | Read stored records |
| `update_platform_fee(new_fee)` | Change fee; admin authorization required |
| `get_platform_fee()` | Read the current fee in basis points |

### Duplicate order protection

`process_payment` treats `order_id` as idempotent per merchant. Before any token transfer it checks whether the `(merchant_id, order_id)` pair has already been paid and panics with `Order already paid` if so, leaving balances, payment records, and merchant statistics untouched. The association is stored as part of the successful payment transaction, and only after the token transfers succeed, so a failed transfer never reserves an order ID and the payer can retry with the same ID.

Order IDs are scoped to a merchant: two different merchants may each use the same `order_id` for their own orders.

## 📡 Events

Each lifecycle event carries exactly one topic — the event name, published as a Soroban string — and a structured data payload. The tables below list every event and every data field the alpha contract emits; no other fields are included.

| Topic | Data |
|---|---|
| `initialized` | `(admin: Address, platform_fee: u32)` |
| `merchant_registered` | `(merchant_id: String, merchant_address: Address)` |
| `payment_processed` | `PaymentProcessedEvent` map |
| `escrow_created` | `EscrowCreatedEvent` map |
| `escrow_released` | `EscrowReleasedEvent` map |
| `escrow_refunded` | `EscrowRefundedEvent` map |
| `fee_updated` | `new_fee: u32` |

The "map" payloads are `#[contracttype]` structs, emitted as a Soroban map keyed by field name, so an indexer can decode them without a storage lookup. Each creation event is published after the token transfers succeed, and each terminal event after the record's status is updated.

### `payment_processed`

Emitted once per successful `process_payment` call, after the payment record, order reservation, and merchant statistics are stored.

| Field | Type | Meaning |
|---|---|---|
| `payment_id` | `u64` | Identifier of the stored payment record |
| `payer` | `Address` | Account that was charged |
| `merchant_id` | `String` | Merchant identifier used in the call |
| `merchant_address` | `Address` | Merchant address that received the net amount |
| `token` | `Address` | Token contract that was transferred |
| `order_id` | `String` | Merchant-scoped order identifier supplied by the caller |
| `amount` | `i128` | Gross amount charged to the payer, before the fee |
| `platform_fee` | `i128` | Portion of `amount` transferred to the admin (0 when the configured fee yields no fee) |

Example (field names as stored in the map, values illustrative):

```json
{
  "payment_id": 1,
  "payer": "G...PAYER",
  "merchant_id": "merchant-1",
  "merchant_address": "G...MERCHANT",
  "token": "C...TOKEN",
  "order_id": "order-1",
  "amount": 10000,
  "platform_fee": 250
}
```

### `escrow_created`

Emitted once per successful `create_escrow` call, after the tokens are held by the contract.

| Field | Type | Meaning |
|---|---|---|
| `escrow_id` | `u64` | Identifier of the stored escrow record |
| `buyer` | `Address` | Account that funded the escrow |
| `seller` | `Address` | Account that receives the funds on release |
| `token` | `Address` | Token contract held in custody |
| `order_id` | `String` | Order identifier supplied by the caller |
| `amount` | `i128` | Amount held in custody |
| `release_time` | `u64` | Unix timestamp at or after which anyone may release |

### `escrow_released`

Emitted once per successful `release_escrow` call, after the funds are transferred to the seller.

| Field | Type | Meaning |
|---|---|---|
| `escrow_id` | `u64` | Identifier of the released escrow |
| `buyer` | `Address` | Buyer recorded on the escrow |
| `seller` | `Address` | Seller that received the funds |
| `token` | `Address` | Token contract transferred |
| `order_id` | `String` | Order identifier recorded on the escrow |
| `amount` | `i128` | Amount released to the seller |

### `escrow_refunded`

Emitted once per successful `refund_escrow` call, after the funds are returned to the buyer.

| Field | Type | Meaning |
|---|---|---|
| `escrow_id` | `u64` | Identifier of the refunded escrow |
| `buyer` | `Address` | Buyer that received the refund |
| `seller` | `Address` | Seller recorded on the escrow |
| `token` | `Address` | Token contract transferred |
| `order_id` | `String` | Order identifier recorded on the escrow |
| `amount` | `i128` | Amount refunded to the buyer |

## 🧪 Testing

```bash
make check
```

`make check` runs formatting, Clippy, and workspace tests. Tests use a local Soroban environment and a test token contract; they do not require a funded account or network access.

## 📁 Project Structure

```
soroban-payment-gateway/
├── contracts/payment_gateway/src/
│   ├── lib.rs                   # Contract entry points and fee calculation
│   ├── payment.rs               # Payment record types
│   ├── escrow.rs                # Escrow record types
│   ├── merchant.rs              # Merchant record type
│   ├── storage.rs               # Typed storage keys and accessors
│   └── test.rs                  # Soroban contract behavior tests
├── Makefile
├── Cargo.toml
├── Cargo.lock
├── .github/workflows/ci.yml
└── README.md
```

## 🤝 Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

### Development Workflow

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Write or update tests
5. Run the test suite (`make test`)
6. Commit your changes (`git commit -m 'Add amazing feature'`)
7. Push to the branch (`git push origin feature/amazing-feature`)
8. Open a Pull Request

## 🗺️ Roadmap

- [x] Basic payment processing
- [x] Escrow functionality
- [x] Merchant registry
- [ ] Multi-signature support
- [ ] Recurring payments/subscriptions
- [ ] Cross-chain bridge integration
- [ ] Mobile SDK (React Native)
- [ ] Payment analytics dashboard

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🔗 Links

- [Developer documentation](https://nexastellar.github.io/soroban-payment-gateway/)
- [Soroban Docs](https://soroban.stellar.org/docs)
- [Stellar Network](https://stellar.org)
- [Report Issues](https://github.com/NexaStellar/soroban-payment-gateway/issues)

The documentation site is maintained in [`docs/`](docs/). GitHub Pages must be enabled with **GitHub Actions** as the build and deployment source.

## 🙏 Acknowledgments

Built for the Stellar ecosystem. NexaStellar intends to apply this repository to the Drips Wave program; approval is pending.

---

**Maintained by NexaStellar** | [Website](https://nexastellar.dev) | [GitHub](https://github.com/NexaStellar)
