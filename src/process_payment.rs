pub async fn process_payment(payment: bytes::Bytes) -> Result<(), Box<dyn std::error::Error>> {
    println!("Processing payment: {:?}", payment);
    // Here you would add your payment processing logic

    Ok(())
}
