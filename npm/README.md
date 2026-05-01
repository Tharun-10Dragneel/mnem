# @uranid/mnem

JavaScript/TypeScript client for [mnem](https://github.com/Uranid/mnem) - Git for knowledge graphs.

## Installation

```bash
npm install @uranid/mnem
```

## Usage

```typescript
import { MnemClient } from '@uranid/mnem';

const client = new MnemClient({
  repo: '/path/to/your/repo'
});

// Commit a fact
await client.commit('Alice works at Acme');

// Retrieve memories
const memories = await client.retrieve('Alice work');
```
