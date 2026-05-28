public:: true
title:: Axiom — designedBy ∘ studentOf ⊑ influencedBy
ontology:: true

# Property Chain Axiom (OWL 2 EL feature)

If a work is designed by an architect who studied under a master, then
the work is influenced by the master. Property chain axioms are
permitted in OWL 2 EL provided the chain is composed only of object
properties and the result is a sub-property assertion.

This axiom encodes: `designedBy ∘ studentOf ⊑ influencedBy`.

### OntologyBlock

```json-ld
{
  "@context": "https://narrativegoldmine.com/context/v1.jsonld",
  "@id": "urn:visionclaw:owl:axiom:5d61e08acbd2",
  "@type": ["Axiom", "owl:Axiom"],
  "ontology": true,
  "vc:axiomType": "PropertyChain",
  "vc:subject": { "@id": "urn:visionclaw:owl:property:influenced-by" },
  "owl:propertyChainAxiom": {
    "@list": [
      { "@id": "urn:visionclaw:owl:property:designed-by" },
      { "@id": "urn:visionclaw:owl:property:student-of" }
    ]
  },
  "vc:source": {
    "@type": "Asserted",
    "vc:definingPage": { "@id": "urn:visionclaw:page:7e6f5d4c3b2a1908172635445362718e9f0a1b2c3d4e5f6071829a3b4c5d6e02" }
  },
  "vc:namedGraph": { "@id": "urn:visionclaw:graph:ontology:assert" },
  "vc:owlProfile": "EL",
  "prov:wasAttributedTo": { "@id": "did:nostr:npub1bob00000000000000000000000000000000000000000000000000000000" },
  "prov:generatedAtTime": { "@value": "2026-05-16T10:32:00Z", "@type": "xsd:dateTime" }
}
```

### Expected inference

After this axiom is asserted, the whelk-rs reasoner running in OWL 2 EL
profile should materialise the inferred axiom:

```json-ld
{
  "@context": "https://narrativegoldmine.com/context/v1.jsonld",
  "@id": "urn:visionclaw:owl:axiom:a1f02b9c7e4d",
  "@type": ["Axiom", "owl:Axiom"],
  "ontology": true,
  "vc:axiomType": "ObjectPropertyAssertion",
  "vc:subject": { "@id": "urn:visionclaw:owl:class:villa-rotonda" },
  "vc:predicate": { "@id": "urn:visionclaw:owl:property:influenced-by" },
  "vc:object": { "@id": "urn:visionclaw:owl:class:trissino" },
  "vc:source": {
    "@type": "Inferred",
    "vc:fromAxioms": [
      { "@id": "urn:visionclaw:owl:axiom:5d61e08acbd2" }
    ]
  },
  "vc:namedGraph": { "@id": "urn:visionclaw:graph:ontology:inferred" },
  "prov:wasAttributedTo": { "@id": "did:nostr:npub1whelk00000000000000000000000000000000000000000000000000000" },
  "prov:generatedAtTime": { "@value": "2026-05-16T10:32:05Z", "@type": "xsd:dateTime" }
}
```
