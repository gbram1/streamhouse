import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  cn,
  formatBytes,
  formatNumber,
  formatCompactNumber,
  formatDuration,
  formatDate,
  formatRelativeTime,
  formatPercent,
  getHealthColor,
  getLagColor,
  calculateLagTrend,
  truncate,
  copyToClipboard,
  downloadCSV,
} from '../utils';

describe('cn', () => {
  it('merges class names', () => {
    expect(cn('foo', 'bar')).toBe('foo bar');
  });

  it('handles conditional classes', () => {
    expect(cn('base', false && 'hidden', 'visible')).toBe('base visible');
  });

  it('merges tailwind conflicts', () => {
    const result = cn('p-4', 'p-6');
    expect(result).toBe('p-6');
  });
});

describe('formatBytes', () => {
  it('returns "0 Bytes" for 0', () => {
    expect(formatBytes(0)).toBe('0 Bytes');
  });

  it('formats bytes', () => {
    expect(formatBytes(500)).toBe('500 Bytes');
  });

  it('formats kilobytes', () => {
    expect(formatBytes(1024)).toBe('1 KB');
  });

  it('formats megabytes', () => {
    expect(formatBytes(1048576)).toBe('1 MB');
  });

  it('formats gigabytes', () => {
    expect(formatBytes(1073741824)).toBe('1 GB');
  });

  it('respects decimals parameter', () => {
    expect(formatBytes(1536, 1)).toBe('1.5 KB');
  });

  it('handles negative decimals as 0', () => {
    expect(formatBytes(1536, -1)).toBe('2 KB');
  });
});

describe('formatNumber', () => {
  it('formats with commas', () => {
    expect(formatNumber(1000)).toBe('1,000');
    expect(formatNumber(1000000)).toBe('1,000,000');
  });

  it('handles small numbers', () => {
    expect(formatNumber(42)).toBe('42');
  });
});

describe('formatCompactNumber', () => {
  it('formats thousands as K', () => {
    expect(formatCompactNumber(1200)).toBe('1.2K');
  });

  it('formats millions as M', () => {
    expect(formatCompactNumber(3400000)).toBe('3.4M');
  });

  it('leaves small numbers as-is', () => {
    expect(formatCompactNumber(42)).toBe('42');
  });
});

describe('formatDuration', () => {
  it('formats milliseconds', () => {
    expect(formatDuration(500)).toBe('500ms');
  });

  it('formats seconds', () => {
    expect(formatDuration(2500)).toBe('2.5s');
  });

  it('formats minutes', () => {
    expect(formatDuration(90000)).toBe('1.5m');
  });

  it('formats hours', () => {
    expect(formatDuration(7200000)).toBe('2.0h');
  });

  it('formats days', () => {
    expect(formatDuration(172800000)).toBe('2.0d');
  });
});

describe('formatDate', () => {
  it('returns N/A for undefined', () => {
    expect(formatDate(undefined)).toBe('N/A');
  });

  it('returns N/A for 0', () => {
    expect(formatDate(0)).toBe('N/A');
  });

  it('returns Invalid Date for bad input', () => {
    expect(formatDate('not-a-date')).toBe('Invalid Date');
  });

  it('formats a valid timestamp', () => {
    const result = formatDate(1700000000000);
    expect(result).toMatch(/2023/);
    expect(result).toMatch(/Nov/);
  });

  it('formats an ISO string', () => {
    const result = formatDate('2024-01-15T10:30:00Z');
    expect(result).toMatch(/2024/);
    expect(result).toMatch(/Jan/);
  });
});

describe('formatRelativeTime', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2024-01-15T12:00:00Z'));
  });

  it('returns "just now" for recent timestamps', () => {
    expect(formatRelativeTime(Date.now() - 30_000)).toBe('just now');
  });

  it('returns minutes ago', () => {
    expect(formatRelativeTime(Date.now() - 5 * 60_000)).toBe('5m ago');
  });

  it('returns hours ago', () => {
    expect(formatRelativeTime(Date.now() - 3 * 3600_000)).toBe('3h ago');
  });

  it('returns days ago', () => {
    expect(formatRelativeTime(Date.now() - 2 * 86400_000)).toBe('2d ago');
  });

  afterEach(() => {
    vi.useRealTimers();
  });
});

describe('formatPercent', () => {
  it('formats as percentage', () => {
    expect(formatPercent(0.856)).toBe('85.6%');
  });

  it('respects decimals', () => {
    expect(formatPercent(0.856, 0)).toBe('86%');
  });
});

describe('getHealthColor', () => {
  it('returns green for healthy', () => {
    expect(getHealthColor('healthy')).toContain('green');
  });

  it('returns yellow for degraded', () => {
    expect(getHealthColor('degraded')).toContain('yellow');
  });

  it('returns red for unhealthy', () => {
    expect(getHealthColor('unhealthy')).toContain('red');
  });
});

describe('getLagColor', () => {
  it('returns green for zero lag', () => {
    expect(getLagColor(0)).toContain('green');
  });

  it('returns yellow for lag below threshold', () => {
    expect(getLagColor(500)).toContain('yellow');
  });

  it('returns red for lag at or above threshold', () => {
    expect(getLagColor(1000)).toContain('red');
    expect(getLagColor(5000)).toContain('red');
  });

  it('respects custom threshold', () => {
    expect(getLagColor(50, 100)).toContain('yellow');
    expect(getLagColor(150, 100)).toContain('red');
  });
});

describe('calculateLagTrend', () => {
  it('returns stable for small changes', () => {
    expect(calculateLagTrend(100, 95)).toBe('stable');
  });

  it('returns increasing for significant increase', () => {
    expect(calculateLagTrend(200, 100)).toBe('increasing');
  });

  it('returns decreasing for significant decrease', () => {
    expect(calculateLagTrend(50, 100)).toBe('decreasing');
  });

  it('handles zero previous lag', () => {
    expect(calculateLagTrend(100, 0)).toBe('increasing');
  });
});

describe('truncate', () => {
  it('returns string unchanged if within length', () => {
    expect(truncate('hello', 10)).toBe('hello');
  });

  it('truncates with ellipsis', () => {
    expect(truncate('hello world', 5)).toBe('hello...');
  });

  it('handles exact length', () => {
    expect(truncate('hello', 5)).toBe('hello');
  });
});

describe('copyToClipboard', () => {
  it('copies text and returns true', async () => {
    const result = await copyToClipboard('test text');
    expect(result).toBe(true);
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('test text');
  });
});

describe('downloadCSV', () => {
  it('does nothing for empty data', () => {
    const createObjectURL = vi.fn();
    global.URL.createObjectURL = createObjectURL;
    downloadCSV([], 'test.csv');
    expect(createObjectURL).not.toHaveBeenCalled();
  });
});
