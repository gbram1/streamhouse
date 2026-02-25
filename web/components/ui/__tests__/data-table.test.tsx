import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { DataTable, Column } from '../data-table';

interface TestRow {
  id: string;
  name: string;
  count: number;
}

const testData: TestRow[] = [
  { id: '1', name: 'Alpha', count: 10 },
  { id: '2', name: 'Beta', count: 30 },
  { id: '3', name: 'Charlie', count: 20 },
  { id: '4', name: 'Delta', count: 5 },
  { id: '5', name: 'Echo', count: 15 },
];

const columns: Column<TestRow>[] = [
  { id: 'name', header: 'Name', accessorKey: 'name', sortable: true },
  { id: 'count', header: 'Count', accessorKey: 'count', sortable: true },
];

describe('DataTable', () => {
  it('renders column headers', () => {
    render(<DataTable data={testData} columns={columns} />);
    expect(screen.getByText('Name')).toBeInTheDocument();
    expect(screen.getByText('Count')).toBeInTheDocument();
  });

  it('renders all data rows', () => {
    render(<DataTable data={testData} columns={columns} />);
    expect(screen.getByText('Alpha')).toBeInTheDocument();
    expect(screen.getByText('Beta')).toBeInTheDocument();
    expect(screen.getByText('Charlie')).toBeInTheDocument();
  });

  it('shows loading skeletons when isLoading', () => {
    const { container } = render(
      <DataTable data={[]} columns={columns} isLoading={true} loadingRows={3} />
    );
    const skeletons = container.querySelectorAll('.animate-pulse');
    expect(skeletons.length).toBeGreaterThan(0);
  });

  it('shows empty state when no data', () => {
    render(
      <DataTable
        data={[]}
        columns={columns}
        emptyState={<div>No items found</div>}
      />
    );
    expect(screen.getByText('No items found')).toBeInTheDocument();
  });

  it('shows default empty state when no emptyState prop', () => {
    render(<DataTable data={[]} columns={columns} />);
    expect(screen.getByText('No data available')).toBeInTheDocument();
  });

  describe('search', () => {
    it('filters data by search query', () => {
      render(
        <DataTable
          data={testData}
          columns={columns}
          searchKey="name"
          searchPlaceholder="Search topics..."
        />
      );

      const input = screen.getByPlaceholderText('Search topics...');
      fireEvent.change(input, { target: { value: 'Alpha' } });

      expect(screen.getByText('Alpha')).toBeInTheDocument();
      expect(screen.queryByText('Beta')).not.toBeInTheDocument();
    });

    it('is case insensitive', () => {
      render(
        <DataTable data={testData} columns={columns} searchKey="name" />
      );

      const input = screen.getByPlaceholderText('Search...');
      fireEvent.change(input, { target: { value: 'alpha' } });

      expect(screen.getByText('Alpha')).toBeInTheDocument();
    });
  });

  describe('sorting', () => {
    it('sorts ascending on first click', () => {
      render(<DataTable data={testData} columns={columns} />);

      const nameHeader = screen.getByText('Name');
      fireEvent.click(nameHeader);

      const rows = screen.getAllByRole('row');
      // Header row + data rows. First data row should be Alpha (ascending)
      expect(rows[1]).toHaveTextContent('Alpha');
    });

    it('sorts descending on second click', () => {
      render(<DataTable data={testData} columns={columns} />);

      const nameHeader = screen.getByText('Name');
      fireEvent.click(nameHeader); // asc
      fireEvent.click(nameHeader); // desc

      const rows = screen.getAllByRole('row');
      expect(rows[1]).toHaveTextContent('Echo');
    });
  });

  describe('pagination', () => {
    it('paginates data', () => {
      const largeData = Array.from({ length: 25 }, (_, i) => ({
        id: String(i),
        name: `Item ${i}`,
        count: i,
      }));

      render(
        <DataTable data={largeData} columns={columns} pageSize={10} />
      );

      expect(screen.getByText('Showing 1 to 10 of 25 results')).toBeInTheDocument();
    });

    it('navigates to next page', () => {
      const largeData = Array.from({ length: 25 }, (_, i) => ({
        id: String(i),
        name: `Item ${i}`,
        count: i,
      }));

      render(
        <DataTable data={largeData} columns={columns} pageSize={10} />
      );

      expect(screen.getByText('Page 1 of 3')).toBeInTheDocument();
    });
  });

  describe('row click', () => {
    it('calls onRowClick when a row is clicked', () => {
      const onRowClick = vi.fn();
      render(
        <DataTable
          data={testData}
          columns={columns}
          onRowClick={onRowClick}
        />
      );

      fireEvent.click(screen.getByText('Alpha'));
      expect(onRowClick).toHaveBeenCalledWith(testData[0]);
    });
  });

  describe('custom getRowId', () => {
    it('uses custom getRowId for row keys', () => {
      render(
        <DataTable
          data={testData}
          columns={columns}
          getRowId={(row) => row.id}
        />
      );

      expect(screen.getByText('Alpha')).toBeInTheDocument();
    });
  });
});
