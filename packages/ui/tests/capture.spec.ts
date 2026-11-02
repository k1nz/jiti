import { describe, expect, it } from 'vitest';
import { isDocumentLocation, isStalePreferredCapture, shouldApplyCapture, shouldApplyDelayedCapture, shouldAutoSubmitOnCapture } from '../src/capture';

describe('shouldApplyCapture', () => {
  it('只接受当前这次热键的捕获', () => {
    expect(shouldApplyCapture(2, 2, false)).toBe(true);
    expect(shouldApplyCapture(1, 2, false)).toBe(false);
  });

  it('用户已经改过输入时不再覆盖', () => {
    expect(shouldApplyCapture(2, 2, true)).toBe(false);
  });
});

describe('isStalePreferredCapture', () => {
  it('AX/UIA 返回与上次相同的文本时视为过期', () => {
    expect(isStalePreferredCapture('hello', 'ax', 'hello')).toBe(true);
    expect(isStalePreferredCapture('hello', 'uia', 'hello')).toBe(true);
  });

  it('新选区或剪贴板结果可以立刻采用', () => {
    expect(isStalePreferredCapture('world', 'ax', 'hello')).toBe(false);
    expect(isStalePreferredCapture('hello', 'clipboard', 'hello')).toBe(false);
    expect(isStalePreferredCapture('', 'ax', 'hello')).toBe(false);
  });
});

describe('isDocumentLocation', () => {
  it('把 Foxmail 读信框地址当成页面地址', () => {
    expect(
      isDocumentLocation(
        'file:///C:/Users/77251/AppData/Roaming/Foxmail7/Temp-54432-20261101174538/mail_frame.html?foxhandler=MailReadProcessHandler&b=2',
      ),
    ).toBe(true);
    expect(isDocumentLocation('https://mail.example.com/read?id=1')).toBe(true);
    expect(isDocumentLocation('about:blank')).toBe(true);
  });

  it('普通选区和夹在句子里的链接不是页面地址', () => {
    expect(isDocumentLocation('Delivery to the following recipient failed')).toBe(false);
    expect(isDocumentLocation('See https://example.com for details')).toBe(false);
    expect(isDocumentLocation('')).toBe(false);
  });
});

describe('shouldApplyDelayedCapture', () => {
  it('输入已有内容时不采用迟到的剪贴板', () => {
    expect(shouldApplyDelayedCapture(2, 2, false, 'already')).toBe(false);
    expect(shouldApplyDelayedCapture(2, 2, false, '')).toBe(true);
    expect(shouldApplyDelayedCapture(1, 2, false, '')).toBe(false);
  });

  it('输入里只有页面地址时允许剪贴板覆盖', () => {
    expect(
      shouldApplyDelayedCapture(
        2,
        2,
        false,
        'file:///C:/Users/77251/AppData/Roaming/Foxmail7/Temp-54432-20261101174538/mail_frame.html?foxhandler=MailReadProcessHandler&b=2',
      ),
    ).toBe(true);
  });
});

describe('shouldAutoSubmitOnCapture', () => {
  it('语法热键拿到非空选区后自动检查', () => {
    expect(shouldAutoSubmitOnCapture('grammar', 'He go to school.')).toBe(true);
    expect(shouldAutoSubmitOnCapture('syntax', 'The book was expensive.')).toBe(true);
    expect(shouldAutoSubmitOnCapture('translate', 'hello')).toBe(true);
    expect(shouldAutoSubmitOnCapture('grammar', '  ')).toBe(false);
    expect(shouldAutoSubmitOnCapture('syntax', '  ')).toBe(false);
    expect(shouldAutoSubmitOnCapture('mistakes', 'He go to school.')).toBe(false);
  });
});
