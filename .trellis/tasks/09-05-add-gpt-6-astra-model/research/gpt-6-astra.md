# GPT-6 Astra model metadata research

## Sources

| Source | Type | Result |
| --- | --- | --- |
| https://developers.openai.com/api/docs/models/gpt-6-astra | OpenAI official model documentation | Loaded in a browser session on 2026-09-05; authoritative values below. Static reader requests returned HTTP 403, so the browser-rendered page text is the evidence used. |
| https://developers.openai.com/api/docs/pricing | OpenAI official pricing documentation | Static reader request returned HTTP 403; the GPT-6 Astra model page contains the model-specific pricing and long-context rule. |

## Confirmed values

The official model page identifies GPT-6 Astra as `gpt-6-astra` and reports:

- Context window: `1,050,000` tokens.
- Maximum output tokens: `128,000`.
- Input modalities: text and image; output modality: text.
- Reasoning effort values: low, medium, high, xhigh, and max.
- Standard text-token prices per 1M tokens:
  - Input: `$10.00` → `10,000,000` micro-USD.
  - Cached input: `$1.00` → `1,000,000` micro-USD.
  - Cache writes: `$12.50` → `12,500,000` micro-USD.
  - Output: `$50.00` → `50,000,000` micro-USD.
- Prompts with more than `272K` input tokens use 2x input and cache rates and 1.5x output for the full request.
- The API page lists both Chat Completions and Responses endpoints and supports streaming, function calling, structured outputs, and the Responses tools shown in the page.

## Catalog mapping

The existing catalog stores integer micro-USD rates and selects the highest tier whose `min_input_tokens` is less than or equal to the request input count. Therefore the official “more than 272K” rule maps to:

- Base tier: `min_input_tokens=0`, rates `10,000,000 / 1,000,000 / 12,500,000 / 50,000,000`.
- Long tier: `min_input_tokens=272001`, rates `20,000,000 / 2,000,000 / 25,000,000 / 75,000,000`.

The model's `context_window` and `max_context_window` are both set to `1,050,000`; the separate `max_output_tokens` capability remains `128,000` so the output limit is not confused with the input context window. Price source is the official model URL above.

## Confidence and limitations

Confidence is high for the numeric values and model ID because they were read from the official OpenAI model page. The static reader and web-search providers were inconsistent or blocked, so no claim is based on the secondary LLM Stats page. The repository's existing capability JSON contains Codex-specific compatibility fields; only fields required by current catalog tests and the official model contract should be carried forward rather than inventing unsupported provider metadata.
