import { useFilesPage } from "@app/contexts/FilesPageContext";
import type { FolderId } from "@app/types/folder";

/** Opens PaperRing library at this folder; the URL drives selection and browser history. */
export function useOpenFolder(): (id: FolderId | null) => void {
  return useFilesPage().openFolder;
}
