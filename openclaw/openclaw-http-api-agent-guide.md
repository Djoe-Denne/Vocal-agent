# OpenClaw HTTP API Reference - Agent Optimized

**Base URL**: `http://127.0.0.1:18789`  
**Authentication**: Bearer token in `Authorization` header  
**Default Port**: `18789`

---

## Authentication

### Header Format
```
Authorization: Bearer YOUR_GATEWAY_TOKEN
```

### Get Token
```bash
# From config file
cat ~/.openclaw/openclaw.json | jq -r '.gateway.auth.token'

# Generate new token
openclaw doctor --generate-gateway-token
```

---

## Core Endpoints

### Health Check
```http
GET /health
```

**Response:**
```json
{
  "status": "ok",
  "version": "2026.2.6",
  "uptime": 3600
}
```

---

### Gateway Status
```http
GET /api/status
Authorization: Bearer TOKEN
```

**Response:**
```json
{
  "status": "running",
  "agents": ["main"],
  "channels": ["whatsapp", "telegram"],
  "uptime": 7200
}
```

---

## OpenAI-Compatible API

### Chat Completions (Non-Streaming)

```http
POST /v1/chat/completions
Content-Type: application/json
Authorization: Bearer TOKEN

{
  "model": "ollama/qwen3-vl:8b",
  "messages": [
    {"role": "system", "content": "You are a helpful assistant."},
    {"role": "user", "content": "Hello!"}
  ],
  "temperature": 0.7,
  "max_tokens": 1000,
  "stream": false
}
```

**Response:**
```json
{
  "id": "chatcmpl-123",
  "object": "chat.completion",
  "created": 1677652288,
  "model": "ollama/qwen3-vl:8b",
  "choices": [{
    "index": 0,
    "message": {
      "role": "assistant",
      "content": "Hello! How can I help you?"
    },
    "finish_reason": "stop"
  }],
  "usage": {
    "prompt_tokens": 15,
    "completion_tokens": 8,
    "total_tokens": 23
  }
}
```

---

### Chat Completions (Streaming)

```http
POST /v1/chat/completions
Content-Type: application/json
Authorization: Bearer TOKEN

{
  "model": "ollama/qwen3-vl:8b",
  "messages": [
    {"role": "user", "content": "Count to 5"}
  ],
  "stream": true
}
```

**Response** (Server-Sent Events):
```
data: {"id":"chatcmpl-123","choices":[{"delta":{"content":"1"},"index":0}]}

data: {"id":"chatcmpl-123","choices":[{"delta":{"content":" 2"},"index":0}]}

data: {"id":"chatcmpl-123","choices":[{"delta":{"content":" 3"},"index":0}]}

data: [DONE]
```

---

### Vision Support (Images)

```http
POST /v1/chat/completions
Content-Type: application/json
Authorization: Bearer TOKEN

{
  "model": "ollama/qwen3-vl:8b",
  "messages": [
    {
      "role": "user",
      "content": [
        {
          "type": "text",
          "text": "What's in this image?"
        },
        {
          "type": "image_url",
          "image_url": {
            "url": "data:image/jpeg;base64,/9j/4AAQSkZJRg..."
          }
        }
      ]
    }
  ],
  "stream": false
}
```

**Image Format**: Base64-encoded JPEG/PNG

---

## Model Selection

### Available Models

Get from config:
```bash
cat ~/.openclaw/openclaw.json | jq '.models.providers'
```

### Model ID Format
```
<provider>/<model-name>

Examples:
- ollama/qwen3-vl:8b
- ollama/llama3.3:70b
- anthropic/claude-sonnet-4
- openai/gpt-4o
```

---

## Request Parameters

### Standard Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `model` | string | required | Model identifier |
| `messages` | array | required | Conversation history |
| `temperature` | float | 0.7 | Randomness (0.0-2.0) |
| `max_tokens` | integer | model default | Max response tokens |
| `top_p` | float | 1.0 | Nucleus sampling |
| `stream` | boolean | false | Enable streaming |
| `stop` | string/array | null | Stop sequences |

### Message Format

```json
{
  "role": "system|user|assistant",
  "content": "text or content array"
}
```

### Content Array (Multimodal)

```json
{
  "role": "user",
  "content": [
    {"type": "text", "text": "What's this?"},
    {"type": "image_url", "image_url": {"url": "data:image/..."}}
  ]
}
```

---

## Error Responses

### Standard Error Format
```json
{
  "error": {
    "message": "Invalid authentication token",
    "type": "authentication_error",
    "code": 401
  }
}
```

### Common Error Codes

| Code | Type | Description |
|------|------|-------------|
| 401 | `authentication_error` | Invalid/missing token |
| 404 | `not_found` | Endpoint not found |
| 400 | `invalid_request` | Malformed request |
| 500 | `server_error` | Internal error |
| 503 | `service_unavailable` | Model/service down |

---

## Rate Limiting

**Default**: No rate limits for local usage  
**Recommendation**: Implement client-side throttling for production

---

## cURL Examples

### Basic Request
```bash
curl http://127.0.0.1:18789/v1/chat/completions \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "ollama/qwen3-vl:8b",
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

### Streaming Request
```bash
curl http://127.0.0.1:18789/v1/chat/completions \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -N \
  -d '{
    "model": "ollama/qwen3-vl:8b",
    "messages": [{"role": "user", "content": "Count to 10"}],
    "stream": true
  }'
```

### Image Analysis
```bash
# Encode image
IMAGE_BASE64=$(base64 -w 0 image.jpg)

curl http://127.0.0.1:18789/v1/chat/completions \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d "{
    \"model\": \"ollama/qwen3-vl:8b\",
    \"messages\": [{
      \"role\": \"user\",
      \"content\": [
        {\"type\": \"text\", \"text\": \"Describe this image\"},
        {\"type\": \"image_url\", \"image_url\": {\"url\": \"data:image/jpeg;base64,$IMAGE_BASE64\"}}
      ]
    }]
  }"
```

---

## Python Client

```python
import requests
import base64

class OpenClawClient:
    def __init__(self, base_url="http://127.0.0.1:18789", token=None):
        self.base_url = base_url
        self.token = token
        self.headers = {
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json"
        }
    
    def chat(self, messages, model="ollama/qwen3-vl:8b", stream=False, **kwargs):
        """Send chat completion request"""
        payload = {
            "model": model,
            "messages": messages,
            "stream": stream,
            **kwargs
        }
        
        response = requests.post(
            f"{self.base_url}/v1/chat/completions",
            headers=self.headers,
            json=payload,
            stream=stream
        )
        
        if stream:
            return response.iter_lines()
        return response.json()
    
    def chat_with_image(self, text, image_path, model="ollama/qwen3-vl:8b"):
        """Send chat request with image"""
        # Encode image
        with open(image_path, "rb") as f:
            image_base64 = base64.b64encode(f.read()).decode()
        
        messages = [{
            "role": "user",
            "content": [
                {"type": "text", "text": text},
                {"type": "image_url", "image_url": {
                    "url": f"data:image/jpeg;base64,{image_base64}"
                }}
            ]
        }]
        
        return self.chat(messages, model)
    
    def health(self):
        """Check gateway health"""
        response = requests.get(f"{self.base_url}/health")
        return response.json()

# Usage
client = OpenClawClient(token="your-token")

# Simple chat
response = client.chat([
    {"role": "user", "content": "Hello!"}
])
print(response["choices"][0]["message"]["content"])

# Chat with image
response = client.chat_with_image("What's in this image?", "photo.jpg")
print(response["choices"][0]["message"]["content"])

# Streaming
for line in client.chat([{"role": "user", "content": "Count to 10"}], stream=True):
    if line:
        print(line.decode())
```

---

## Node.js Client

```javascript
const axios = require('axios');
const fs = require('fs');

class OpenClawClient {
  constructor(baseURL = 'http://127.0.0.1:18789', token = null) {
    this.client = axios.create({
      baseURL,
      headers: {
        'Authorization': `Bearer ${token}`,
        'Content-Type': 'application/json'
      }
    });
  }

  async chat(messages, options = {}) {
    const payload = {
      model: options.model || 'ollama/qwen3-vl:8b',
      messages,
      stream: options.stream || false,
      ...options
    };

    const response = await this.client.post('/v1/chat/completions', payload);
    return response.data;
  }

  async chatWithImage(text, imagePath, model = 'ollama/qwen3-vl:8b') {
    const imageBuffer = fs.readFileSync(imagePath);
    const imageBase64 = imageBuffer.toString('base64');

    const messages = [{
      role: 'user',
      content: [
        { type: 'text', text },
        { 
          type: 'image_url',
          image_url: { url: `data:image/jpeg;base64,${imageBase64}` }
        }
      ]
    }];

    return this.chat(messages, { model });
  }

  async health() {
    const response = await this.client.get('/health');
    return response.data;
  }
}

// Usage
const client = new OpenClawClient('http://127.0.0.1:18789', 'your-token');

(async () => {
  // Simple chat
  const response = await client.chat([
    { role: 'user', content: 'Hello!' }
  ]);
  console.log(response.choices[0].message.content);

  // Chat with image
  const imageResponse = await client.chatWithImage(
    "What's in this image?",
    'photo.jpg'
  );
  console.log(imageResponse.choices[0].message.content);
})();
```

---

## Configuration Reference

### Minimal Config for HTTP API

```json
{
  "gateway": {
    "port": 18789,
    "bind": "loopback",
    "auth": {
      "mode": "token",
      "token": "your-secure-token"
    }
  },
  "models": {
    "providers": {
      "ollama": {
        "baseUrl": "http://127.0.0.1:11434/v1",
        "apiKey": "ollama",
        "api": "openai-responses",
        "models": [
          {
            "id": "qwen3-vl:8b",
            "name": "Qwen3 VL",
            "input": ["text", "image"],
            "contextWindow": 32768,
            "maxTokens": 8192
          }
        ]
      }
    }
  },
  "agents": {
    "defaults": {
      "model": {
        "primary": "ollama/qwen3-vl:8b"
      }
    }
  }
}
```

**Location**: `~/.openclaw/openclaw.json`

---

## Environment Variables

```bash
# Gateway settings
export OPENCLAW_GATEWAY_PORT=18789
export OPENCLAW_GATEWAY_BIND=loopback
export OPENCLAW_GATEWAY_TOKEN=your-token

# Model provider (if using cloud)
export ANTHROPIC_API_KEY=sk-ant-...
export OPENAI_API_KEY=sk-...
```

---

## Testing & Debugging

### Quick Health Check
```bash
curl http://127.0.0.1:18789/health
```

### Test Authentication
```bash
TOKEN=$(cat ~/.openclaw/openclaw.json | jq -r '.gateway.auth.token')
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:18789/api/status
```

### Test Chat Completion
```bash
TOKEN=$(cat ~/.openclaw/openclaw.json | jq -r '.gateway.auth.token')
curl http://127.0.0.1:18789/v1/chat/completions \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "ollama/qwen3-vl:8b",
    "messages": [{"role": "user", "content": "test"}],
    "max_tokens": 10
  }' | jq
```

---

## Agent Integration Patterns

### Autonomous Agent Loop

```python
import time

client = OpenClawClient(token="your-token")

def agent_loop():
    context = []
    
    while True:
        # Get task from queue/user
        task = get_next_task()
        
        # Add to context
        context.append({"role": "user", "content": task})
        
        # Get response
        response = client.chat(context)
        assistant_message = response["choices"][0]["message"]
        
        # Add to context
        context.append(assistant_message)
        
        # Execute actions if needed
        if should_execute(assistant_message):
            execute_action(assistant_message)
        
        # Prune context if too long
        if len(context) > 20:
            context = context[-20:]
        
        time.sleep(1)
```

### Multi-Turn Conversation

```python
conversation = []

def chat_turn(user_input):
    conversation.append({"role": "user", "content": user_input})
    
    response = client.chat(
        messages=conversation,
        temperature=0.7,
        max_tokens=500
    )
    
    assistant_message = response["choices"][0]["message"]
    conversation.append(assistant_message)
    
    return assistant_message["content"]

# Usage
print(chat_turn("Hello, I need help with a task"))
print(chat_turn("The task is to analyze an image"))
```

### Function Calling Pattern

```python
def chat_with_tools(user_message, tools=[]):
    response = client.chat([
        {"role": "system", "content": "You have access to tools. Use them when needed."},
        {"role": "user", "content": user_message}
    ])
    
    message = response["choices"][0]["message"]
    
    # Check if tool should be called
    if "TOOL:" in message["content"]:
        # Parse and execute tool
        tool_result = execute_tool(message["content"])
        
        # Send result back
        return client.chat([
            {"role": "user", "content": user_message},
            {"role": "assistant", "content": message["content"]},
            {"role": "user", "content": f"Tool result: {tool_result}"}
        ])
    
    return message["content"]
```

---

## Security Notes

1. **Token Storage**: Never hardcode tokens in code
2. **HTTPS**: Use reverse proxy (nginx/Caddy) for production
3. **Binding**: Keep `bind: "loopback"` unless exposing to network
4. **Rate Limiting**: Implement application-level throttling
5. **Input Validation**: Sanitize all user inputs before sending to API

---

## Troubleshooting

| Issue | Check | Solution |
|-------|-------|----------|
| Connection refused | `curl http://127.0.0.1:18789/health` | Verify gateway running |
| 401 Unauthorized | Token in config | Regenerate token |
| Model not found | `openclaw config show` | Verify model ID |
| Timeout | Gateway logs | Increase request timeout |
| No response | Check model provider | Verify Ollama/provider running |

---

## Quick Reference

```bash
# Get token
cat ~/.openclaw/openclaw.json | jq -r '.gateway.auth.token'

# Check health
curl http://127.0.0.1:18789/health

# Test chat
curl http://127.0.0.1:18789/v1/chat/completions \
  -H "Authorization: Bearer TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"ollama/qwen3-vl:8b","messages":[{"role":"user","content":"hi"}]}'

# View logs
openclaw logs -f

# Restart gateway
openclaw gateway restart
```

---

**API Version**: Compatible with OpenAI API v1  
**OpenClaw Version**: 2026.2.6+  
**Last Updated**: February 2026
