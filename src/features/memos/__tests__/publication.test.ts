import { describe, expect, it } from 'vitest';
import { canSwitchGeneration, commonPlayableBoundary, reducePeaks, selectWaveformLevel } from '../publication';

describe('memo publication',()=>{
  it('publishes only a common contiguous rendition boundary',()=>{
    expect(commonPlayableBoundary([
      {rendition:32,sequence:0,durationMs:1000},{rendition:64,sequence:0,durationMs:990},
      {rendition:32,sequence:1,durationMs:1000},{rendition:32,sequence:2,durationMs:1000},{rendition:64,sequence:2,durationMs:1000},
    ])).toEqual({sequence:0,durationMs:990});
  });

  it('selects a duration-appropriate waveform level and preserves silence',()=>{
    expect(selectWaveformLevel([1,10,100],180_000,1000)).toBe(100);
    expect(selectWaveformLevel([1,10,100],250,500)).toBe(1);
    expect(reducePeaks([0,0,4,2],2)).toEqual({step:2,peaks:[0,4]});
  });

  it('keeps the published generation until a retry catches up',()=>{
    expect(canSwitchGeneration(3,4,12_000,11_999)).toBe(false);
    expect(canSwitchGeneration(3,4,12_000,12_000)).toBe(true);
    expect(canSwitchGeneration(0,1,0,1_000)).toBe(true);
  });
});
