import OpenAI from 'openai'
import { spawn } from 'child_process'
import { relative } from 'path'
import { config } from './config.js'
import { type SupportedChatModel as SupportedChatModelType } from './schema.js'
import { logCliDebug } from './logger.js'

// Map short model names to OpenRouter's prefixed format
const OPENROUTER_MODEL_MAP: Record<string, string> = {
  'gemini-2.5-pro': 'google/gemini-2.5-pro',
  'gemini-3-pro-preview': 'google/gemini-3-pro-preview',
  'deepseek-reasoner': 'deepseek/deepseek-reasoner',
  'gpt-5.2': 'openai/gpt-5.2',
  'gpt-5.3-codex': 'openai/gpt-5.3-codex',
  'gpt-5.2-codex': 'openai/gpt-5.2-codex',
  'gpt-5.1-codex-max': 'openai/gpt-5.1-codex-max',
  'gpt-5.1-codex': 'openai/gpt-5.1-codex',
  'gpt-5.1-codex-mini': 'openai/gpt-5.1-codex-mini',
  'gpt-5.1': 'openai/gpt-5.1',
}

function toOpenRouterModel(model: string): string {
  return OPENROUTER_MODEL_MAP[model] ?? model
}

export interface LlmExecutor {
  execute(
    prompt: string,
    model: SupportedChatModelType,
    systemPrompt: string,
    filePaths?: string[],
  ): Promise<{
    response: string
    usage: OpenAI.CompletionUsage | null
  }>
}

/**
 * Creates an executor that interacts with an OpenAI-compatible API.
 *
 * Don't let it confuse you that client is of type OpenAI. We used OpenAI API
 * client for Gemini also.
 */
function createApiExecutor(
  client: OpenAI,
  options?: { mapModel?: (model: string) => string },
): LlmExecutor {
  return {
    async execute(prompt, model, systemPrompt, filePaths) {
      if (filePaths && filePaths.length > 0) {
        // Explicitly reject unsupported parameters
        console.warn(
          `Warning: File paths were provided but are not supported by the API executor for model ${model}. They will be ignored.`,
        )
      }

      const resolvedModel = options?.mapModel ? options.mapModel(model) : model

      const completion = await client.chat.completions.create({
        model: resolvedModel,
        messages: [
          { role: 'system', content: systemPrompt },
          { role: 'user', content: prompt },
        ],
      })

      const response = completion.choices[0]?.message?.content
      if (!response) {
        throw new Error('No response from the model via API')
      }

      return { response, usage: completion.usage ?? null }
    },
  }
}

/**
 * Configuration for a command-line tool executor.
 */
type CliConfig = {
  cliName: string
  buildArgs: (model: SupportedChatModelType, fullPrompt: string) => string[]
  handleNonZeroExit: (code: number, stderr: string) => Error
}

/**
 * Creates an executor that delegates to a command-line tool.
 */
function createCliExecutor(cliConfig: CliConfig): LlmExecutor {
  const buildFullPrompt = (
    prompt: string,
    systemPrompt: string,
    filePaths?: string[],
  ): string => {
    let fullPrompt = `${systemPrompt}\n\n${prompt}`
    if (filePaths && filePaths.length > 0) {
      const fileReferences = filePaths
        .map((path) => `@${relative(process.cwd(), path)}`)
        .join(' ')
      fullPrompt = `${fullPrompt}\n\nFiles: ${fileReferences}`
    }
    return fullPrompt
  }

  return {
    async execute(prompt, model, systemPrompt, filePaths) {
      const fullPrompt = buildFullPrompt(prompt, systemPrompt, filePaths)
      const args = cliConfig.buildArgs(model, fullPrompt)
      const { cliName } = cliConfig

      return new Promise((resolve, reject) => {
        try {
          logCliDebug(`Spawning ${cliName} CLI`, {
            model,
            promptLength: fullPrompt.length,
            filePathsCount: filePaths?.length || 0,
            args: args,
            promptPreview: fullPrompt.slice(0, 300),
          })

          const child = spawn(cliName, args, {
            shell: false,
            stdio: ['ignore', 'pipe', 'pipe'],
          })

          let stdout = ''
          let stderr = ''
          const startTime = Date.now()

          child.on('spawn', () =>
            logCliDebug(`${cliName} CLI process spawned successfully`),
          )

          child.stdout.on('data', (data: Buffer) => (stdout += data.toString()))
          child.stderr.on('data', (data: Buffer) => (stderr += data.toString()))

          child.on('close', (code) => {
            const duration = Date.now() - startTime

            logCliDebug(`${cliName} CLI process closed`, {
              code,
              duration: `${duration}ms`,
              stdoutLength: stdout.length,
              stderrLength: stderr.length,
            })

            if (code === 0) {
              resolve({ response: stdout.trim(), usage: null })
            } else {
              reject(cliConfig.handleNonZeroExit(code ?? -1, stderr))
            }
          })

          child.on('error', (err) => {
            logCliDebug(`Failed to spawn ${cliName} CLI`, {
              error: err.message,
            })
            reject(
              new Error(
                `Failed to spawn ${cliName} CLI. Is it installed and in PATH? Error: ${err.message}`,
              ),
            )
          })
        } catch (err) {
          reject(
            new Error(
              `Synchronous error while trying to spawn ${cliName}: ${
                err instanceof Error ? err.message : String(err)
              }`,
            ),
          )
        }
      })
    },
  }
}

// --- CLI Configurations ---
const geminiCliConfig: CliConfig = {
  cliName: 'gemini',
  buildArgs: (model, fullPrompt) => ['-m', model, '-p', fullPrompt],
  handleNonZeroExit: (code, stderr) => {
    if (stderr.includes('RESOURCE_EXHAUSTED')) {
      return new Error(
        `Gemini quota exceeded. Consider using gemini-2.0-flash model. Error: ${stderr.trim()}`,
      )
    }
    return new Error(
      `Gemini CLI exited with code ${code}. Error: ${stderr.trim()}`,
    )
  },
}

const codexCliConfig: CliConfig = {
  cliName: 'codex',
  buildArgs: (model, fullPrompt) => {
    const args = ['exec', '--skip-git-repo-check', '-m', model]
    if (config.codexReasoningEffort) {
      args.push('-c', `model_reasoning_effort="${config.codexReasoningEffort}"`)
    }
    args.push(fullPrompt)
    return args
  },
  handleNonZeroExit: (code, stderr) =>
    new Error(`Codex CLI exited with code ${code}. Error: ${stderr.trim()}`),
}

const createExecutorProvider = () => {
  const executorCache = new Map<string, LlmExecutor>()
  const clientCache = new Map<string, OpenAI>()

  const getOpenRouterClient = (): OpenAI => {
    if (clientCache.has('openrouter')) return clientCache.get('openrouter')!
    if (!config.openrouterApiKey) {
      throw new Error(
        'OPENROUTER_API_KEY environment variable is required for OpenRouter mode',
      )
    }
    const client = new OpenAI({
      apiKey: config.openrouterApiKey,
      baseURL: 'https://openrouter.ai/api/v1',
    })
    clientCache.set('openrouter', client)
    return client
  }

  const getOpenAIClient = (): OpenAI => {
    if (clientCache.has('openai')) return clientCache.get('openai')!
    if (!config.openaiApiKey) {
      throw new Error(
        'OPENAI_API_KEY environment variable is required for OpenAI models in API mode',
      )
    }
    const client = new OpenAI({ apiKey: config.openaiApiKey })
    clientCache.set('openai', client)
    return client
  }

  const getDeepseekClient = (): OpenAI => {
    if (clientCache.has('deepseek')) return clientCache.get('deepseek')!
    if (!config.deepseekApiKey) {
      throw new Error(
        'DEEPSEEK_API_KEY environment variable is required for DeepSeek models',
      )
    }
    const client = new OpenAI({
      apiKey: config.deepseekApiKey,
      baseURL: 'https://api.deepseek.com',
    })
    clientCache.set('deepseek', client)
    return client
  }

  const getGeminiApiClient = (): OpenAI => {
    if (clientCache.has('geminiApi')) return clientCache.get('geminiApi')!
    if (!config.geminiApiKey) {
      throw new Error(
        'GEMINI_API_KEY environment variable is required for Gemini models in API mode',
      )
    }
    const client = new OpenAI({
      apiKey: config.geminiApiKey,
      baseURL: 'https://generativelanguage.googleapis.com/v1beta/openai/',
    })
    clientCache.set('geminiApi', client)
    return client
  }

  return (model: SupportedChatModelType): LlmExecutor => {
    // When OpenRouter is configured, route all models through it
    const useOpenRouter = !!config.openrouterApiKey

    // Create cache key that includes mode for models that support CLI
    const cacheKey = useOpenRouter
      ? `openrouter-${model}`
      : model +
        (model.startsWith('gpt-') ? `-${config.openaiMode}` : '') +
        (model.startsWith('gemini-') ? `-${config.geminiMode}` : '')

    if (executorCache.has(cacheKey)) {
      return executorCache.get(cacheKey)!
    }

    let executor: LlmExecutor

    if (useOpenRouter) {
      // OpenRouter handles all models via a single OpenAI-compatible endpoint
      executor = createApiExecutor(getOpenRouterClient(), {
        mapModel: toOpenRouterModel,
      })
    } else if (model.startsWith('gpt-')) {
      executor =
        config.openaiMode === 'cli'
          ? createCliExecutor(codexCliConfig)
          : createApiExecutor(getOpenAIClient())
    } else if (model.startsWith('deepseek-')) {
      executor = createApiExecutor(getDeepseekClient())
    } else if (model.startsWith('gemini-')) {
      executor =
        config.geminiMode === 'cli'
          ? createCliExecutor(geminiCliConfig)
          : createApiExecutor(getGeminiApiClient())
    } else {
      throw new Error(`Unable to determine LLM provider for model: ${model}`)
    }

    executorCache.set(cacheKey, executor)
    return executor
  }
}

export const getExecutorForModel = createExecutorProvider()
