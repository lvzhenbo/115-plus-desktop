export interface Aria2Response<T> {
  jsonrpc: string;
  id: string;
  result: T;
}

export interface Aria2Task {
  gid: string;
  status: 'active' | 'waiting' | 'paused' | 'complete' | 'error' | 'removed';
  totalLength: string;
  completedLength: string;
  downloadSpeed: string;
  files: {
    path: string;
  }[];
}
