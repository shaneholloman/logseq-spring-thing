/**
 * WAC Manager
 *
 * Web Access Control (WAC) ACL serialisation and write operations for
 * Solid pods. Handles building Turtle ACL documents and PUTting them to
 * the server's .acl companion resources.
 */

import { createLogger } from '../../utils/loggerConfig';
import { fetchWithAuth, resolvePath } from './ldpClient';

const logger = createLogger('SolidPodService:wac');

export type AclMode = 'Read' | 'Write' | 'Append' | 'Control';

export interface AclEntry {
  /** Subject WebID or public-access flag */
  agentWebId: string;
  /** WAC access modes to grant */
  modes: ('Read' | 'Write' | 'Append')[];
}

/**
 * Build a WAC Turtle ACL document that:
 *  - Grants the owner full control (Read, Write, Control)
 *  - Grants the agent the specified modes
 *
 * Both grants apply to the container and its default (inherited) children.
 */
export function buildAclTurtle(
  containerUrl: string,
  ownerWebId: string,
  agentEntry: AclEntry
): string {
  const agentModes = agentEntry.modes.map((m) => `acl:${m}`).join(', ');

  return `@prefix acl: <http://www.w3.org/ns/auth/acl#>.
@prefix foaf: <http://xmlns.com/foaf/0.1/>.

# Owner retains full control
<#owner>
    a acl:Authorization;
    acl:agent <${ownerWebId}>;
    acl:accessTo <${containerUrl}>;
    acl:default <${containerUrl}>;
    acl:mode acl:Read, acl:Write, acl:Control.

# Agent access
<#agent>
    a acl:Authorization;
    acl:agent <${agentEntry.agentWebId}>;
    acl:accessTo <${containerUrl}>;
    acl:default <${containerUrl}>;
    acl:mode ${agentModes}.
`;
}

/**
 * Write a WAC ACL document for the given container path.
 *
 * The ACL resource is placed at `{containerPath}.acl` following the
 * Solid/WAC convention.
 *
 * @param containerPath - Pod-relative or absolute container path
 * @param ownerWebId    - WebID of the pod owner (retains full control)
 * @param agentEntry    - Agent WebID and modes to grant
 * @returns true if the ACL was written successfully
 */
export async function writeContainerAcl(
  containerPath: string,
  ownerWebId: string,
  agentEntry: AclEntry
): Promise<boolean> {
  const resolvedContainer = resolvePath(containerPath);
  const aclUrl = resolvePath(`${containerPath}.acl`);
  const aclTurtle = buildAclTurtle(resolvedContainer, ownerWebId, agentEntry);

  const response = await fetchWithAuth(aclUrl, {
    method: 'PUT',
    headers: { 'Content-Type': 'text/turtle' },
    body: aclTurtle,
  });

  if (!response.ok) {
    logger.error('Failed to write ACL', { containerPath, status: response.status });
    return false;
  }

  logger.info('ACL updated', {
    containerPath,
    agentWebId: agentEntry.agentWebId,
    modes: agentEntry.modes,
  });
  return true;
}
