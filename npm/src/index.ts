/**
 * @uranid/mnem - JavaScript client for mnem
 */

export class MnemClient {
  constructor(options: { repo: string }) {
    this.repo = options.repo;
  }
  
  private repo: string;
  
  async commit(content: string): Promise<string> {
    // TODO: Implement MCP client call
    throw new Error('Not implemented');
  }
  
  async retrieve(query: string, options?: { limit?: number }): Promise<string[]> {
    // TODO: Implement MCP client call
    throw new Error('Not implemented');
  }
}

export default { MnemClient };
