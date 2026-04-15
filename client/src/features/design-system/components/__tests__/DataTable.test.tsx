import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import React from 'react';
import { DataTable } from '../DataTable';

interface TestRow {
  id: number;
  name: string;
  score: number;
  optional?: string | null;
}

const columns = [
  { key: 'id', header: 'ID', sortable: true },
  { key: 'name', header: 'Name', sortable: true },
  { key: 'score', header: 'Score', sortable: true },
];

const sampleData: TestRow[] = [
  { id: 1, name: 'Alice', score: 90 },
  { id: 2, name: 'Bob', score: 75 },
  { id: 3, name: 'Charlie', score: 85 },
];

describe('DataTable', () => {
  it('renders column headers', () => {
    render(<DataTable data={sampleData} columns={columns} />);
    expect(screen.getByText('ID')).toBeTruthy();
    expect(screen.getByText('Name')).toBeTruthy();
    expect(screen.getByText('Score')).toBeTruthy();
  });

  it('renders data rows', () => {
    render(<DataTable data={sampleData} columns={columns} />);
    expect(screen.getByText('Alice')).toBeTruthy();
    expect(screen.getByText('Bob')).toBeTruthy();
    expect(screen.getByText('Charlie')).toBeTruthy();
    expect(screen.getByText('90')).toBeTruthy();
    expect(screen.getByText('75')).toBeTruthy();
  });

  it('shows empty message when no data', () => {
    render(<DataTable data={[]} columns={columns} />);
    expect(screen.getByText('No data')).toBeTruthy();
  });

  it('shows custom empty message', () => {
    render(<DataTable data={[]} columns={columns} emptyMessage="Nothing here" />);
    expect(screen.getByText('Nothing here')).toBeTruthy();
  });

  it('toggles sorting asc then desc then none on column click', () => {
    render(<DataTable data={sampleData} columns={columns} />);
    const nameHeader = screen.getByText('Name');

    // Click 1: asc
    fireEvent.click(nameHeader);
    const rows1 = screen.getAllByRole('row');
    // row 0 is header, rows 1-3 are data
    expect(rows1[1].textContent).toContain('Alice');
    expect(rows1[2].textContent).toContain('Bob');
    expect(rows1[3].textContent).toContain('Charlie');

    // Click 2: desc
    fireEvent.click(nameHeader);
    const rows2 = screen.getAllByRole('row');
    expect(rows2[1].textContent).toContain('Charlie');
    expect(rows2[2].textContent).toContain('Bob');
    expect(rows2[3].textContent).toContain('Alice');

    // Click 3: none (back to original order)
    fireEvent.click(nameHeader);
    const rows3 = screen.getAllByRole('row');
    expect(rows3[1].textContent).toContain('Alice');
    expect(rows3[2].textContent).toContain('Bob');
    expect(rows3[3].textContent).toContain('Charlie');
  });

  it('only sortable columns respond to clicks', () => {
    const mixedColumns = [
      { key: 'id', header: 'ID', sortable: false },
      { key: 'name', header: 'Name', sortable: true },
    ];
    const data = [
      { id: 2, name: 'Bob' },
      { id: 1, name: 'Alice' },
    ];
    render(<DataTable data={data} columns={mixedColumns} />);

    // Click non-sortable column - order unchanged
    fireEvent.click(screen.getByText('ID'));
    const rows = screen.getAllByRole('row');
    expect(rows[1].textContent).toContain('Bob');
    expect(rows[2].textContent).toContain('Alice');

    // Click sortable column - order changes
    fireEvent.click(screen.getByText('Name'));
    const rowsSorted = screen.getAllByRole('row');
    expect(rowsSorted[1].textContent).toContain('Alice');
    expect(rowsSorted[2].textContent).toContain('Bob');
  });

  it('fires onRowClick with correct row data', () => {
    const handleRowClick = vi.fn();
    render(<DataTable data={sampleData} columns={columns} onRowClick={handleRowClick} />);

    const rows = screen.getAllByRole('row');
    fireEvent.click(rows[2]); // second data row = Bob
    expect(handleRowClick).toHaveBeenCalledWith(sampleData[1]);
  });

  it('uses custom render function when provided', () => {
    const customColumns = [
      {
        key: 'name',
        header: 'Name',
        render: (row: TestRow) => <span data-testid="custom-render">{row.name.toUpperCase()}</span>,
      },
    ];
    render(<DataTable data={sampleData} columns={customColumns} />);
    const customCells = screen.getAllByTestId('custom-render');
    expect(customCells[0].textContent).toBe('ALICE');
    expect(customCells[1].textContent).toBe('BOB');
  });

  it('handles null and undefined values in sort', () => {
    const dataWithNulls: TestRow[] = [
      { id: 1, name: 'Alice', score: 90, optional: 'yes' },
      { id: 2, name: 'Bob', score: 75, optional: null },
      { id: 3, name: 'Charlie', score: 85, optional: undefined },
    ];
    const nullColumns = [
      { key: 'optional', header: 'Optional', sortable: true },
      { key: 'name', header: 'Name' },
    ];
    // Should not throw when sorting a column with null/undefined values
    render(<DataTable data={dataWithNulls} columns={nullColumns} />);
    fireEvent.click(screen.getByText('Optional'));
    // null/undefined values sort to end
    const rows = screen.getAllByRole('row');
    expect(rows[1].textContent).toContain('yes');
  });
});
