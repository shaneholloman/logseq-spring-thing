import type { GraphData } from '../../managers/graphDataManager';
import type { GraphMetrics } from './types';

export function calculateAveragePathLength(graphData: GraphData): number {
  const { nodes, edges } = graphData;
  if (nodes.length < 2 || edges.length === 0) return 0;

  const adj = new Map<string, Set<string>>();
  nodes.forEach(n => adj.set(n.id, new Set()));
  edges.forEach(e => {
    adj.get(e.source)?.add(e.target);
    adj.get(e.target)?.add(e.source);
  });

  const sampleSize = Math.min(50, nodes.length);
  const step = Math.max(1, Math.floor(nodes.length / sampleSize));
  let totalDist = 0;
  let pairCount = 0;

  for (let si = 0; si < nodes.length; si += step) {
    const start = nodes[si].id;
    const dist = new Map<string, number>([[start, 0]]);
    const queue = [start];
    let head = 0;
    while (head < queue.length) {
      const cur = queue[head++];
      const d = dist.get(cur)!;
      for (const nb of adj.get(cur) || []) {
        if (!dist.has(nb)) { dist.set(nb, d + 1); queue.push(nb); }
      }
    }
    dist.forEach((d, id) => {
      if (id !== start && d > 0) { totalDist += d; pairCount++; }
    });
  }
  return pairCount > 0 ? totalDist / pairCount : 0;
}

export function calculateClusteringCoefficient(graphData: GraphData): number {
  const { nodes, edges } = graphData;
  if (nodes.length < 3) return 0;

  const adj = new Map<string, Set<string>>();
  nodes.forEach(n => adj.set(n.id, new Set()));
  edges.forEach(e => {
    adj.get(e.source)?.add(e.target);
    adj.get(e.target)?.add(e.source);
  });

  let totalCoeff = 0;
  let qualifying = 0;
  adj.forEach(neighbors => {
    const k = neighbors.size;
    if (k < 2) return;
    const nbArr = Array.from(neighbors);
    let triangles = 0;
    for (let i = 0; i < nbArr.length; i++) {
      for (let j = i + 1; j < nbArr.length; j++) {
        if (adj.get(nbArr[i])?.has(nbArr[j])) triangles++;
      }
    }
    totalCoeff += (2 * triangles) / (k * (k - 1));
    qualifying++;
  });
  return qualifying > 0 ? totalCoeff / qualifying : 0;
}

export function calculateCentralization(graphData: GraphData): number {
  const n = graphData.nodes.length;
  if (n < 3) return 0;

  const degree = new Map<string, number>();
  graphData.nodes.forEach(nd => degree.set(nd.id, 0));
  graphData.edges.forEach(e => {
    degree.set(e.source, (degree.get(e.source) || 0) + 1);
    degree.set(e.target, (degree.get(e.target) || 0) + 1);
  });

  const degrees = Array.from(degree.values());
  const maxDeg = Math.max(...degrees);
  const sumDiff = degrees.reduce((s, d) => s + (maxDeg - d), 0);
  return sumDiff / ((n - 1) * (n - 2));
}

export function calculateModularity(graphData: GraphData): number {
  const { nodes, edges } = graphData;
  if (nodes.length < 2 || edges.length === 0) return 0;

  const adj = new Map<string, Set<string>>();
  nodes.forEach(n => adj.set(n.id, new Set()));
  edges.forEach(e => {
    adj.get(e.source)?.add(e.target);
    adj.get(e.target)?.add(e.source);
  });

  // Detect communities via connected components
  const community = new Map<string, number>();
  let cId = 0;
  nodes.forEach(n => {
    if (community.has(n.id)) return;
    const queue = [n.id];
    let head = 0;
    community.set(n.id, cId);
    while (head < queue.length) {
      for (const nb of adj.get(queue[head++]) || []) {
        if (!community.has(nb)) { community.set(nb, cId); queue.push(nb); }
      }
    }
    cId++;
  });

  if (cId <= 1) return 0;

  const m = edges.length;
  const comEdges = new Map<number, number>();
  const comDegree = new Map<number, number>();
  edges.forEach(e => {
    const cs = community.get(e.source)!;
    const ct = community.get(e.target)!;
    if (cs === ct) comEdges.set(cs, (comEdges.get(cs) || 0) + 1);
    comDegree.set(cs, (comDegree.get(cs) || 0) + 1);
    comDegree.set(ct, (comDegree.get(ct) || 0) + 1);
  });

  let Q = 0;
  for (let c = 0; c < cId; c++) {
    const ecc = (comEdges.get(c) || 0) / m;
    const ac = (comDegree.get(c) || 0) / (2 * m);
    Q += ecc - ac * ac;
  }
  return Q;
}

export function calculateNetworkEfficiency(graphData: GraphData): number {
  const { nodes, edges } = graphData;
  if (nodes.length < 2 || edges.length === 0) return 0;

  const adj = new Map<string, Set<string>>();
  nodes.forEach(n => adj.set(n.id, new Set()));
  edges.forEach(e => {
    adj.get(e.source)?.add(e.target);
    adj.get(e.target)?.add(e.source);
  });

  const n = nodes.length;
  const sampleSize = Math.min(50, n);
  const step = Math.max(1, Math.floor(n / sampleSize));
  let totalEff = 0;
  let pairCount = 0;

  for (let si = 0; si < n; si += step) {
    const start = nodes[si].id;
    const dist = new Map<string, number>([[start, 0]]);
    const queue = [start];
    let head = 0;
    while (head < queue.length) {
      const cur = queue[head++];
      const d = dist.get(cur)!;
      for (const nb of adj.get(cur) || []) {
        if (!dist.has(nb)) { dist.set(nb, d + 1); queue.push(nb); }
      }
    }
    dist.forEach((d, id) => {
      if (id !== start && d > 0) { totalEff += 1 / d; pairCount++; }
    });
  }
  return pairCount > 0 ? totalEff / pairCount : 0;
}

export function calculateSmallWorldness(clustering: number, pathLength: number): number {
  return clustering / pathLength;
}

export function computeGraphMetrics(graphData: GraphData): GraphMetrics {
  const nodes = graphData.nodes.length;
  const edges = graphData.edges.length;
  const maxEdges = nodes * (nodes - 1) / 2;

  const density = maxEdges > 0 ? edges / maxEdges : 0;
  const averagePathLength = calculateAveragePathLength(graphData);
  const clusteringCoefficient = calculateClusteringCoefficient(graphData);
  const centralization = calculateCentralization(graphData);
  const modularity = calculateModularity(graphData);
  const efficiency = calculateNetworkEfficiency(graphData);
  const smallWorldness = calculateSmallWorldness(clusteringCoefficient, averagePathLength);

  return { density, averagePathLength, clusteringCoefficient, centralization, modularity, efficiency, smallWorldness };
}
