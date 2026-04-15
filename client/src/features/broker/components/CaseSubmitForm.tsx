import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '../../design-system/components';
import { Button } from '../../design-system/components';
import { Input } from '../../design-system/components';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../../design-system/components';
import { Textarea } from '../../design-system/components';
import { Label } from '../../design-system/components';

interface CaseSubmitFormProps {
  onSubmitted?: () => void;
}

export function CaseSubmitForm({ onSubmitted }: CaseSubmitFormProps) {
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [priority, setPriority] = useState('medium');
  const [submitting, setSubmitting] = useState(false);
  const [result, setResult] = useState<{ id: string } | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!title.trim()) return;

    setSubmitting(true);
    try {
      const response = await fetch('/api/broker/cases', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ title, description, priority, source: 'manual_submission' }),
      });
      const data = await response.json();
      setResult(data);
      setTitle('');
      setDescription('');
      setPriority('medium');
      setTimeout(() => onSubmitted?.(), 1500);
    } catch (err) {
      console.error('Failed to submit case:', err);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Submit Case for Review</CardTitle>
        <CardDescription>
          Describe a shadow workflow, edge case, or coordination issue that needs broker judgment.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="case-title">Title</Label>
            <Input
              id="case-title"
              placeholder="e.g., Data team using unapproved LLM for report generation"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              required
            />
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="case-description">Description</Label>
            <Textarea
              id="case-description"
              placeholder="Describe the workflow pattern, affected teams, and why it needs governance review..."
              value={description}
              onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) => setDescription(e.target.value)}
              rows={4}
            />
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="case-priority">Priority</Label>
            <Select value={priority} onValueChange={setPriority}>
              <SelectTrigger id="case-priority" className="w-[180px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="critical">Critical</SelectItem>
                <SelectItem value="high">High</SelectItem>
                <SelectItem value="medium">Medium</SelectItem>
                <SelectItem value="low">Low</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div className="flex items-center gap-3">
            <Button type="submit" disabled={!title.trim() || submitting}>
              {submitting ? 'Submitting...' : 'Submit Case'}
            </Button>
            {result && (
              <span className="text-sm text-green-400">
                Case {result.id} created
              </span>
            )}
          </div>
        </form>
      </CardContent>
    </Card>
  );
}
