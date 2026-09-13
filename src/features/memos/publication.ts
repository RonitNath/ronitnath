export function commonPlayableBoundary(rows: ReadonlyArray<{ rendition: number; sequence: number; durationMs: number }>) {
  const low=new Map(rows.filter((row)=>row.rendition===32).map((row)=>[row.sequence,row.durationMs]));
  const high=new Map(rows.filter((row)=>row.rendition===64).map((row)=>[row.sequence,row.durationMs]));
  let durationMs=0,sequence=-1;
  while(low.has(sequence+1)&&high.has(sequence+1)){
    sequence+=1;
    durationMs+=Math.min(low.get(sequence)!,high.get(sequence)!);
  }
  return {sequence,durationMs};
}

export function canSwitchGeneration(currentGeneration:number,nextGeneration:number,currentThroughMs:number,nextThroughMs:number){
  return nextThroughMs>0&&(currentGeneration===0||currentGeneration===nextGeneration||nextThroughMs>=currentThroughMs);
}

export function selectWaveformLevel(levels: readonly number[], sourcePeaks: number, columns: number) {
  const ordered=[...levels].sort((a,b)=>a-b);
  return ordered.find((level)=>Math.ceil(sourcePeaks/level)<=columns*2)??ordered.at(-1)??1;
}

export function reducePeaks(peaks: readonly number[], columns: number) {
  const step=Math.max(1,Math.ceil(peaks.length/Math.max(1,columns)));
  return {step,peaks:Array.from({length:Math.ceil(peaks.length/step)},(_,index)=>Math.max(0,...peaks.slice(index*step,(index+1)*step)))};
}
