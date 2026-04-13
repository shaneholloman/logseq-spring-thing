import { useEffect, useState } from 'react';
import { graphDataPort } from '../ports';
import { useSettingsStore } from '../../store/settingsStore';
import type { GraphData } from '../../features/graph/managers/graphDataManager';
import { createLogger } from '../../utils/loggerConfig';

const logger = createLogger('useImmersiveData');


export const useImmersiveData = (initialData?: any) => {
  const [graphData, setGraphData] = useState<GraphData | null>(null);
  const [nodePositions, setNodePositions] = useState<Float32Array | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<string | null>(null);

  
  
  const botsData = null;

  
  const settings = useSettingsStore(state => state.settings);

  useEffect(() => {
    logger.debug('Setting up data subscriptions');

    const subscriptions: (() => void)[] = [];

    try {
      
      const graphDataUnsubscribe = graphDataPort.onGraphDataChange((data: GraphData) => {
        logger.debug('Graph data updated:', data);
        setGraphData(data);
        setIsLoading(false);
        setError(null);
      });
      subscriptions.push(graphDataUnsubscribe);


      const positionUnsubscribe = graphDataPort.onPositionUpdate((positions: Float32Array) => {
        logger.debug('Position data updated, length:', positions?.length);
        setNodePositions(positions);
      });
      subscriptions.push(positionUnsubscribe);


      const initializeData = async () => {
        try {


          const currentData = await graphDataPort.getGraphData();
          if (currentData && currentData.nodes && currentData.nodes.length > 0) {
            logger.debug('Initial graph data available:', currentData);
            setGraphData(currentData);
            setIsLoading(false);
          } else {
            logger.debug('Waiting for graph data...');
            setIsLoading(false); 
            
          }
        } catch (err) {
          logger.error('Failed to get initial graph data:', err);
          
          
          setIsLoading(false);
        }
      };

      initializeData();

    } catch (err) {
      logger.error('Error setting up data subscriptions:', err);
      setError('Failed to set up data connections');
      setIsLoading(false);
    }

    return () => {
      
      subscriptions.forEach(unsub => {
        if (typeof unsub === 'function') {
          unsub();
        }
      });
    };
  }, []);

  const updateNodePosition = (nodeId: string, position: { x: number; y: number; z: number }) => {
    const numericId = graphDataPort.getNodeNumericId(nodeId);
    if (numericId !== undefined) {
      graphDataPort.pinNode(numericId);
      graphDataPort.updateNodePosition(numericId, position);
    }
  };

  const selectNode = (nodeId: string | null) => {
    setSelectedNode(nodeId);
    if (nodeId) {
      logger.debug('Node selected:', nodeId);
    }
  };

  const pinNode = (nodeId: string) => {
    const numericId = graphDataPort.getNodeNumericId(nodeId);
    if (numericId !== undefined) {
      graphDataPort.pinNode(numericId);
    }
  };

  const unpinNode = (nodeId: string) => {
    const numericId = graphDataPort.getNodeNumericId(nodeId);
    if (numericId !== undefined) {
      graphDataPort.unpinNode(numericId);
    }
  };

  return {
    graphData,
    nodePositions,
    botsData,
    settings,
    isLoading,
    error,
    selectedNode,
    updateNodePosition,
    selectNode,
    pinNode,
    unpinNode
  };
};