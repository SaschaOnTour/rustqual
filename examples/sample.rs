#![allow(dead_code)]
// examples/sample.rs
// A sample file with IOSP-compliant and non-compliant functions.

// ──────────────────────────────────────────
// OPERATION: Pure logic, no own function calls. ✓
// ──────────────────────────────────────────
fn calculate_discount(price: f64, percentage: f64) -> f64 {
    let discount = price * percentage / 100.0;
    if discount > price {
        price
    } else {
        discount
    }
}

fn validate_email(email: &str) -> bool {
    let has_at = email.contains('@');
    let has_dot = email.contains('.');
    let not_empty = !email.is_empty();
    has_at && has_dot && not_empty
}

// ──────────────────────────────────────────
// INTEGRATION: Orchestrates calls, no own logic. ✓
// ──────────────────────────────────────────
fn process_order(order: &Order) -> Result<Receipt, String> {
    let validated = validate_order(order)?;
    let price = calculate_total(&validated);
    let discount = calculate_discount(price, validated.discount_pct);
    let final_price = apply_discount(price, discount);
    let receipt = create_receipt(&validated, final_price);
    send_confirmation(&receipt);
    Ok(receipt)
}

fn handle_user_registration(input: &RegistrationInput) -> Result<User, String> {
    let sanitized = sanitize_input(input);
    let valid = validate_registration(&sanitized)?;
    let user = create_user(&valid);
    send_welcome_email(&user);
    Ok(user)
}

// ──────────────────────────────────────────
// VIOLATION: Mixes logic AND own function calls. ✗
// ──────────────────────────────────────────
fn process_payment(order: &Order) -> Result<Payment, String> {
    // Logic: conditional
    if order.total <= 0.0 {
        return Err("Invalid total".to_string());
    }

    // Own function call
    let method = determine_payment_method(order);

    // More logic
    if method == PaymentMethod::CreditCard {
        let fee = order.total * 0.03;
        // Another own call inside a branch — classic IOSP violation
        charge_credit_card(order.total + fee)
    } else {
        process_bank_transfer(order.total)
    }
}

fn generate_report(data: &[Record]) -> String {
    let mut result = String::new();

    // Own call
    let header = build_report_header(data);
    result.push_str(&header);

    // Logic mixed in
    for record in data {
        if record.is_active {
            let line = format_record(record); // own call inside loop
            result.push_str(&line);
        }
    }

    result
}

// ──────────────────────────────────────────
// TRIVIAL: single expression / delegation
// ──────────────────────────────────────────
struct Thing {
    name: String,
}

impl Thing {
    fn get_name(&self) -> &str {
        &self.name
    }
}

// ──────────────────────────────────────────
// Stub types and functions to make the sample parseable
// ──────────────────────────────────────────
struct Order {
    total: f64,
    discount_pct: f64,
}
struct Receipt;
struct RegistrationInput;
struct User;
struct Payment;
struct Record {
    is_active: bool,
}
#[derive(PartialEq)]
enum PaymentMethod {
    CreditCard,
    BankTransfer,
}

fn validate_order(_: &Order) -> Result<Order, String> {
    todo!()
}
fn calculate_total(_: &Order) -> f64 {
    todo!()
}
fn apply_discount(_: f64, _: f64) -> f64 {
    todo!()
}
fn create_receipt(_: &Order, _: f64) -> Receipt {
    todo!()
}
fn send_confirmation(_: &Receipt) {
    todo!()
}
fn sanitize_input(_: &RegistrationInput) -> RegistrationInput {
    todo!()
}
fn validate_registration(_: &RegistrationInput) -> Result<RegistrationInput, String> {
    todo!()
}
fn create_user(_: &RegistrationInput) -> User {
    todo!()
}
fn send_welcome_email(_: &User) {
    todo!()
}
fn determine_payment_method(_: &Order) -> PaymentMethod {
    todo!()
}
fn charge_credit_card(_: f64) -> Result<Payment, String> {
    todo!()
}
fn process_bank_transfer(_: f64) -> Result<Payment, String> {
    todo!()
}
fn build_report_header(_: &[Record]) -> String {
    todo!()
}
fn format_record(_: &Record) -> String {
    todo!()
}

fn main() {}
