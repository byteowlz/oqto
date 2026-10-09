/**
 * Opaque, stable compositor identities (ADR-0041/0043). Container, track,
 * and Arrangement ids are minted deterministically from the snapshot's id
 * seed; Content ids are caller-provided stable identities (a second
 * presentation of the same owner requires an explicitly minted distinct
 * identity). All brands erase to plain strings on the wire.
 */

declare const containerIdBrand: unique symbol;
declare const contentIdBrand: unique symbol;
declare const trackIdBrand: unique symbol;
declare const arrangementIdBrand: unique symbol;

export type ContainerId = string & { readonly [containerIdBrand]: true };
export type ContentId = string & { readonly [contentIdBrand]: true };
export type GridTrackId = string & { readonly [trackIdBrand]: true };
export type ArrangementId = string & { readonly [arrangementIdBrand]: true };
export type LayoutRevision = number;

export function containerIdFrom(seed: number): ContainerId {
	return `container-${seed}` as ContainerId;
}

export function trackIdFrom(seed: number): GridTrackId {
	return `track-${seed}` as GridTrackId;
}

export function arrangementIdFrom(seed: number): ArrangementId {
	return `arrangement-${seed}` as ArrangementId;
}

export function contentIdFrom(identity: string): ContentId {
	return identity as ContentId;
}
