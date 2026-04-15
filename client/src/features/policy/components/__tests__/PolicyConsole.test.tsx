import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import React from 'react';
import { PolicyConsole } from '../PolicyConsole';

/**
 * Helper: Radix Tabs use onMouseDown (not onClick) to trigger tab switches.
 * fireEvent.click does not trigger the internal Radix handler, so we use mouseDown.
 */
function switchTab(name: string) {
  const tab = screen.getByRole('tab', { name });
  fireEvent.mouseDown(tab);
}

describe('PolicyConsole', () => {
  it('renders all 6 default rules', () => {
    render(<PolicyConsole />);
    expect(screen.getByText('Confidence Threshold')).toBeTruthy();
    expect(screen.getByText('Separation of Duty')).toBeTruthy();
    expect(screen.getByText('Domain Ownership')).toBeTruthy();
    expect(screen.getByText('Deployment Scope Limit')).toBeTruthy();
    expect(screen.getByText('Agent Rate Limit')).toBeTruthy();
    expect(screen.getByText('Escalation Cascade')).toBeTruthy();
  });

  it('renders active rule count badge', () => {
    render(<PolicyConsole />);
    // 4 of 6 are enabled by default
    expect(screen.getByText('4/6 active')).toBeTruthy();
  });

  it('toggle switch enables/disables rules', () => {
    render(<PolicyConsole />);
    const switches = screen.getAllByRole('switch');
    expect(switches.length).toBe(6);

    // Toggle the 4th switch (Deployment Scope Limit, disabled by default) on
    fireEvent.click(switches[3]);
    expect(screen.getByText('5/6 active')).toBeTruthy();

    // Toggle it off again
    fireEvent.click(switches[3]);
    expect(screen.getByText('4/6 active')).toBeTruthy();
  });

  it('test bench tab renders evaluate button', () => {
    render(<PolicyConsole />);
    switchTab('Test Bench');
    expect(screen.getByRole('button', { name: 'Evaluate' })).toBeTruthy();
  });

  it('test bench evaluates workflow.promote with low confidence to escalate', () => {
    render(<PolicyConsole />);
    switchTab('Test Bench');

    // Default slider is 50 (confidence=0.50), default action is workflow.promote
    // Confidence Threshold rule has threshold=0.7, so 0.50 < 0.70 => escalate
    fireEvent.click(screen.getByRole('button', { name: 'Evaluate' }));

    // The outcome text is lowercase in the DOM, CSS `uppercase` handles display
    expect(screen.getByText('escalate')).toBeTruthy();
    expect(screen.getByText(/Confidence 0\.50 < threshold 0\.7/)).toBeTruthy();
  });

  it('test bench evaluates with high confidence to allow', () => {
    render(<PolicyConsole />);
    switchTab('Test Bench');

    // Move the Radix Slider to max using keyboard End key (value becomes 100, confidence=1.00)
    const slider = screen.getByRole('slider');
    fireEvent.keyDown(slider, { key: 'End' });

    fireEvent.click(screen.getByRole('button', { name: 'Evaluate' }));

    expect(screen.getByText('allow')).toBeTruthy();
    expect(screen.getByText(/Confidence 1\.00 >= threshold 0\.7/)).toBeTruthy();
  });

  it('evaluation appears in log tab', () => {
    render(<PolicyConsole />);

    // Evaluate in test bench first
    switchTab('Test Bench');
    fireEvent.click(screen.getByRole('button', { name: 'Evaluate' }));

    // Switch to log tab
    switchTab(/Evaluation Log/);

    // The log entry should contain the evaluated action
    expect(screen.getByText('workflow.promote')).toBeTruthy();
  });

  it('empty evaluation log shows guidance message', () => {
    render(<PolicyConsole />);

    // Switch to log tab without evaluating anything first
    switchTab('Evaluation Log');

    expect(screen.getByText('No evaluations yet')).toBeTruthy();
    expect(screen.getByText(/Use the Test Bench to simulate/)).toBeTruthy();
  });
});
