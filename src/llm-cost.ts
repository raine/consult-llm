import { CompletionUsage } from 'openai/resources.js'
import { type SupportedChatModel } from './schema.js'

// Model pricing data
type ModelPricing = {
  inputCostPerMillion: number
  outputCostPerMillion: number
}

const MODEL_PRICING: Partial<Record<SupportedChatModel, ModelPricing>> = {
  'gpt-5.2': {
    inputCostPerMillion: 1.75,
    outputCostPerMillion: 14.0,
  },
  'gemini-2.5-pro': {
    inputCostPerMillion: 1.25,
    outputCostPerMillion: 10.0,
  },
  'gemini-3-pro-preview': {
    inputCostPerMillion: 2.0,
    outputCostPerMillion: 12.0,
  },
  'deepseek-v4-pro': {
    inputCostPerMillion: 0.435,
    outputCostPerMillion: 0.87,
  },
  'deepseek-v4-flash': {
    inputCostPerMillion: 0.14,
    outputCostPerMillion: 0.28,
  },
}

export function calculateCost(
  usage: CompletionUsage | undefined,
  model: SupportedChatModel,
): { inputCost: number; outputCost: number; totalCost: number } {
  const pricing = MODEL_PRICING[model]
  if (!pricing) {
    return { inputCost: 0, outputCost: 0, totalCost: 0 }
  }

  const inputTokens = usage?.prompt_tokens || 0
  const outputTokens = usage?.completion_tokens || 0
  const inputCost = (inputTokens / 1_000_000) * pricing.inputCostPerMillion
  const outputCost = (outputTokens / 1_000_000) * pricing.outputCostPerMillion
  const totalCost = inputCost + outputCost

  return { inputCost, outputCost, totalCost }
}
