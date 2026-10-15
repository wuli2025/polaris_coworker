export const protocolOptions = [
  { value: '', label: 'Anthropic Messages' },
  { value: 'openai', label: 'OpenAI Completions' },
  { value: 'responses', label: 'OpenAI Responses' },
  { value: 'chatgpt', label: 'ChatGPT / Codex Responses' },
  { value: 'google', label: 'Google Generative AI' },
] as const;
export const modelConfigExample = JSON.stringify({
  env: { ANTHROPIC_BASE_URL: 'https://gateway.example/v1' },
  models: ['model-a', 'model-b'],
}, null, 2);

function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
/** Optional models preserve the backend's env/catalog fallback. Validate explicit overrides. */
export function validateProviderConfig(config: unknown, protocol: string): string | null {
  if (!['', 'anthropic', 'openai', 'responses', 'chatgpt', 'google'].includes(protocol)) return '不支持的提供商协议';
  if (!object(config)) return 'settingsConfig 必须是 JSON 对象';
  if (config.env !== undefined && !object(config.env)) return 'env 必须是对象';
  if (config.models === undefined) return null;
  if (!Array.isArray(config.models) || !config.models.length) return 'models 必须是非空数组';
  for (const model of config.models) {
    if (typeof model === 'string' && model.trim()) continue;
    if (!object(model) || typeof model.id !== 'string' || !model.id.trim()) return '每个模型需要非空 ID';
    if (model.name !== undefined && typeof model.name !== 'string') return '模型 name 必须是字符串';
    if (model.reasoning !== undefined && typeof model.reasoning !== 'boolean') return 'reasoning 必须是布尔值';
    for (const field of ['contextWindow', 'maxTokens']) {
      if (model[field] !== undefined && (!Number.isSafeInteger(model[field]) || Number(model[field]) <= 0)) return `${field} 必须是正整数`;
    }
    if (model.input !== undefined && (!Array.isArray(model.input) || !model.input.length ||
      model.input.some((v: unknown) => v !== 'text' && v !== 'image'))) return 'input 只支持 text / image 非空数组';
    if (model.cost !== undefined) {
      if (!object(model.cost)) return 'cost 必须包含 input/output/cacheRead/cacheWrite 四项价格';
      for (const key of ['input', 'output', 'cacheRead', 'cacheWrite']) {
        const price = model.cost[key];
        if (typeof price !== 'number' || !Number.isFinite(price) || price < 0) return '模型价格必须是有限非负数';
      }
    }
  }
  return null;
}
