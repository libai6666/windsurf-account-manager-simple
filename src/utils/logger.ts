import { invoke } from '@tauri-apps/api/core';

interface LogEntry {
  timestamp: string;
  level: 'DEBUG' | 'INFO' | 'WARN' | 'ERROR';
  module: string;
  message: string;
  data?: any;
}

class Logger {
  private logs: LogEntry[] = [];
  private maxLogs = 1000;
  
  private formatTimestamp(): string {
    const now = new Date();
    return now.toISOString().replace('T', ' ').substring(0, 23);
  }
  
  private async writeToFile(entry: LogEntry) {
    try {
      const logLine = `[${entry.timestamp}] [${entry.level}] [${entry.module}] ${entry.message}${entry.data ? ' | ' + JSON.stringify(entry.data) : ''}\n`;
      await invoke('append_log_file', { content: logLine });
    } catch (e) {
      console.error('[Logger] Failed to write log:', e);
    }
  }
  
  private log(level: LogEntry['level'], module: string, message: string, data?: any) {
    const entry: LogEntry = {
      timestamp: this.formatTimestamp(),
      level,
      module,
      message,
      data
    };
    
    // 控制台输出
    const consoleMsg = `[${entry.module}] ${entry.message}`;
    switch (level) {
      case 'DEBUG':
        console.debug(consoleMsg, data || '');
        break;
      case 'INFO':
        console.info(consoleMsg, data || '');
        break;
      case 'WARN':
        console.warn(consoleMsg, data || '');
        break;
      case 'ERROR':
        console.error(consoleMsg, data || '');
        break;
    }
    
    // 保存到内存
    this.logs.push(entry);
    if (this.logs.length > this.maxLogs) {
      this.logs.shift();
    }
    
    // 写入文件
    this.writeToFile(entry);
  }
  
  debug(module: string, message: string, data?: any) {
    this.log('DEBUG', module, message, data);
  }
  
  info(module: string, message: string, data?: any) {
    this.log('INFO', module, message, data);
  }
  
  warn(module: string, message: string, data?: any) {
    this.log('WARN', module, message, data);
  }
  
  error(module: string, message: string, data?: any) {
    this.log('ERROR', module, message, data);
  }
  
  getLogs(): LogEntry[] {
    return [...this.logs];
  }
  
  clear() {
    this.logs = [];
  }
}

export const logger = new Logger();
export default logger;
