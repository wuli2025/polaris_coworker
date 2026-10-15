import type { PriceCompleteness } from '../../types/usageCosts';

export function pricedCost(cost: number): string {
  return Number.isFinite(cost) && cost >= 0 ? `$${cost.toFixed(4)}` : '未知';
}
export function unknownCosts(value: Partial<PriceCompleteness>): string {
  if (typeof value.unpricedRequests !== 'number' || typeof value.hasUnknownCosts !== 'boolean') {
    return '价格完整性未知';
  }
  return value.hasUnknownCosts ? `另有 ${value.unpricedRequests} 次请求未标价` : '全部请求已标价';
}
