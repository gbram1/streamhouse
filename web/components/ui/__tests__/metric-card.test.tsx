import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MetricCard } from '../metric-card';
import { Activity } from 'lucide-react';

describe('MetricCard', () => {
  it('renders title and value', () => {
    render(<MetricCard title="Total Topics" value={42} />);
    expect(screen.getByText('Total Topics')).toBeInTheDocument();
    expect(screen.getByText('42')).toBeInTheDocument();
  });

  it('renders string value', () => {
    render(<MetricCard title="Status" value="Healthy" />);
    expect(screen.getByText('Healthy')).toBeInTheDocument();
  });

  it('renders description when provided', () => {
    render(
      <MetricCard title="Messages" value="1.2M" description="Last 24 hours" />
    );
    expect(screen.getByText('Last 24 hours')).toBeInTheDocument();
  });

  it('does not render description when not provided', () => {
    render(<MetricCard title="Test" value={0} />);
    expect(screen.queryByText('Last 24 hours')).not.toBeInTheDocument();
  });

  it('renders positive trend in green', () => {
    render(
      <MetricCard
        title="Throughput"
        value="50K/s"
        trend={{ value: 12, label: 'vs last hour' }}
      />
    );
    const trendEl = screen.getByText('+12%');
    expect(trendEl).toBeInTheDocument();
    expect(trendEl.className).toContain('green');
  });

  it('renders negative trend in red', () => {
    render(
      <MetricCard
        title="Latency"
        value="5ms"
        trend={{ value: -8, label: 'vs last hour' }}
      />
    );
    const trendEl = screen.getByText('-8%');
    expect(trendEl).toBeInTheDocument();
    expect(trendEl.className).toContain('red');
  });

  it('renders zero trend with muted styling', () => {
    render(
      <MetricCard
        title="Errors"
        value={0}
        trend={{ value: 0, label: 'vs last hour' }}
      />
    );
    const trendEl = screen.getByText('0%');
    expect(trendEl.className).toContain('muted');
  });

  it('renders trend label', () => {
    render(
      <MetricCard
        title="Test"
        value={1}
        trend={{ value: 5, label: 'vs yesterday' }}
      />
    );
    expect(screen.getByText('vs yesterday')).toBeInTheDocument();
  });

  it('applies custom className', () => {
    const { container } = render(
      <MetricCard title="Test" value={1} className="custom-class" />
    );
    expect(container.firstChild).toHaveClass('custom-class');
  });
});
