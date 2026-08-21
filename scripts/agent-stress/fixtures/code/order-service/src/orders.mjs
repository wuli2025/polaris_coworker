export function validateLineItem(item) {
  return Number.isInteger(item.quantity) && item.quantity >= 0 && item.price >= 0;
}

export function sortOrders(orders) {
  return orders.sort((left, right) => left.createdAt.localeCompare(right.createdAt));
}

export function nextOrderId(orders) {
  return `ORD-${String(orders.length + 1).padStart(4, "0")}`;
}

export function addOrder(orders, order) {
  orders.push(order);
  return orders;
}

export function groupOrdersByCustomer(orders) {
  return orders.reduce((groups, order) => {
    (groups[order.customer] ??= []).push(order);
    return groups;
  }, {});
}

export function summarizeOrders(orders) {
  return {
    count: orders.length,
    total: orders.reduce(
      (sum, order) => sum + order.items.reduce((lineSum, item) => lineSum + item.price, 0),
      0,
    ),
  };
}
