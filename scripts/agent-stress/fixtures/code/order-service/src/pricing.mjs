export function discountedTotal(items, percent) {
  const subtotal = items.reduce((sum, item) => sum + item.price * item.quantity, 0);
  return Math.round(subtotal * (1 - percent));
}

export function calculateTax(amount, region) {
  const rate = region === "JP" ? 0.13 : 0;
  return Math.round(amount * rate);
}
