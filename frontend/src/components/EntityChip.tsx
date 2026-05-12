import { Badge } from "./ui/badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import type { EntityType, MemoryEntity } from "../api/types";

type EntityChipProps = {
  entity: MemoryEntity;
};

export function EntityChip({ entity }: EntityChipProps) {
  const type = normalizeEntityType(entity.entity_type);

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge variant={entityVariant(type)} tabIndex={0} className="focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent">
          {entity.value}
        </Badge>
      </TooltipTrigger>
      <TooltipContent>{typeLabel(type)}</TooltipContent>
    </Tooltip>
  );
}

export function normalizeEntityType(value: string): EntityType {
  const normalized = value.toLowerCase();
  if (normalized === "person" || normalized === "repo" || normalized === "branch" || normalized === "file" || normalized === "team") {
    return normalized;
  }

  return "topic";
}

function entityVariant(type: EntityType): "blue" | "purple" | "green" | "amber" | "teal" | "gray" {
  if (type === "person") {
    return "blue";
  }
  if (type === "repo") {
    return "purple";
  }
  if (type === "branch") {
    return "green";
  }
  if (type === "file") {
    return "amber";
  }
  if (type === "team") {
    return "teal";
  }
  return "gray";
}

function typeLabel(type: EntityType): string {
  return type.charAt(0).toUpperCase() + type.slice(1);
}
