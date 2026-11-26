'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '../ui/dialog';
import { Button } from '../ui/button';
import { Input } from '../ui/input';

export function RenameSpeakerDialog({
  meetingId,
  label,
  current,
  onClose,
  onSaved,
}: {
  meetingId: string;
  label: string;
  current: string;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [name, setName] = useState(current);
  const [roster, setRoster] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    invoke<string[]>('api_get_roster_names').then(setRoster).catch(() => setRoster([]));
  }, []);

  const save = async () => {
    setSaving(true);
    try {
      await invoke('api_rename_speaker', { meetingId, label, name });
      onSaved();
      onClose();
    } catch (error) {
      console.error('保存说话人名称失败:', error);
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>重命名说话人</DialogTitle>
        </DialogHeader>
        <Input
          value={name}
          list="roster-names"
          placeholder="输入姓名，留空则恢复默认"
          onChange={(event) => setName(event.target.value)}
        />
        <datalist id="roster-names">
          {roster.map((person) => <option key={person} value={person} />)}
        </datalist>
        <div className="flex justify-end gap-2">
          <Button variant="outline" onClick={onClose}>取消</Button>
          <Button variant="blue" disabled={saving} onClick={() => void save()}>保存</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
