export interface Aria2Response<T> {
  jsonrpc: string;
  id: string;
  result: T;
  error?: {
    code: number;
    message: string;
  };
}

export interface Aria2Task {
  gid: string;
  status: 'active' | 'waiting' | 'paused' | 'complete' | 'error' | 'removed';
  totalLength: string;
  completedLength: string;
  downloadSpeed: string;
  files: {
    path: string;
    length: string;
    completedLength: string;
    uris: { uri: string; status: string }[];
  }[];
  errorCode?: string;
  errorMessage?: string;
}

export interface Aria2GlobalStat {
  downloadSpeed: string;
  uploadSpeed: string;
  numActive: string;
  numWaiting: string;
  numStopped: string;
  numStoppedTotal: string;
}
