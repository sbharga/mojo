PDFLATEX := pdflatex
BIBTEX := bibtex
JOB := mojo-engine
SOURCE := main.tex

.PHONY: all clean

all: $(JOB).pdf

$(JOB).pdf: $(SOURCE) references.bib data/fixed-depth.csv data/timed-depth.csv data/artifacts.csv
	$(PDFLATEX) -interaction=nonstopmode -halt-on-error -jobname=$(JOB) $(SOURCE)
	$(BIBTEX) $(JOB)
	$(PDFLATEX) -interaction=nonstopmode -halt-on-error -jobname=$(JOB) $(SOURCE)
	$(PDFLATEX) -interaction=nonstopmode -halt-on-error -jobname=$(JOB) $(SOURCE)

clean:
	rm -f $(JOB).aux $(JOB).bbl $(JOB).blg $(JOB).log $(JOB).out \
		$(JOB).toc $(JOB).lof $(JOB).lot $(JOB).pdf
