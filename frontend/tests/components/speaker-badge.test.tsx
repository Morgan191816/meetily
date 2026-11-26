import { afterEach, describe, expect, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import { SpeakerBadge, speakerText } from '../../src/components/Transcript/SpeakerBadge';

let renderer: ReactTestRenderer | undefined;
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
});

describe('speakerText', () => {
  test('无显示名时按标签编号回退', () => {
    expect(speakerText('SPEAKER_00', null)).toBe('说话人 1');
    expect(speakerText('SPEAKER_03')).toBe('说话人 4');
  });

  test('有显示名时优先显示并去空白', () => {
    expect(speakerText('SPEAKER_01', '曹磊')).toBe('曹磊');
    expect(speakerText('SPEAKER_01', '  曹磊  ')).toBe('曹磊');
    expect(speakerText('SPEAKER_01', '   ')).toBe('说话人 2');
  });

  test('非编号标签回退为说话人 1', () => {
    expect(speakerText('未知')).toBe('说话人 1');
  });
});

describe('SpeakerBadge', () => {
  test('渲染显示名与配色', async () => {
    await act(async () => {
      renderer = create(<SpeakerBadge label="SPEAKER_00" displayName="曹磊" />);
    });

    const tree = renderer!.toJSON() as any;
    expect(tree.type).toBe('button');
    expect(tree.children).toEqual(['曹磊']);
    expect(tree.props.className).toContain('bg-blue-100');
    expect(tree.props.className).toContain('cursor-default');
  });

  test('调色板按下标回绕，不越界', async () => {
    await act(async () => {
      renderer = create(<SpeakerBadge label="SPEAKER_09" />);
    });

    const className = (renderer!.toJSON() as any).props.className as string;
    // 9 % 8 = 1，回绕到第二项，而不是 undefined
    expect(className).toContain('bg-emerald-100');
    expect(className).not.toContain('undefined');
  });

  test('可点击时触发回调并显示手型光标', async () => {
    let clicked = 0;
    await act(async () => {
      renderer = create(<SpeakerBadge label="SPEAKER_02" onClick={() => { clicked += 1; }} />);
    });

    const tree = renderer!.toJSON() as any;
    expect(tree.props.className).toContain('cursor-pointer');
    await act(async () => {
      tree.props.onClick();
    });
    expect(clicked).toBe(1);
  });
});
