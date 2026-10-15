export interface PriceCompleteness {
  unpricedRequests: number;
  hasUnknownCosts: boolean;
}
export interface ModelCost {
  source: string;
  model: string;
  hasPrice: boolean;
  pricedRequests: number;
  unpricedRequests: number;
  cost: number;
}
export interface BucketCosts extends PriceCompleteness { models: ModelCost[] }
