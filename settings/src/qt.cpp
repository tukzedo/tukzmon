// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// The settings window in Qt Widgets. Rows come from settings.shcl through the
// Rust side, and every change goes back there, so this file only lays out
// controls and copies state onto them. Same model as SilkTerm's dialog: tabs
// down the left, a revert arrow per row, and Cancel, Apply, Apply and close.

#include "tukzmon-settings/src/qt.h"
#include "tukzmon-settings/src/bridge.rs.h"

#include <QApplication>
#include <QCheckBox>
#include <QColorDialog>
#include <QComboBox>
#include <QDialog>
#include <QDoubleSpinBox>
#include <QFrame>
#include <QKeyEvent>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QMessageBox>
#include <QPixmap>
#include <QPushButton>
#include <QScreen>
#include <QRegularExpressionValidator>
#include <QScrollArea>
#include <QSignalBlocker>
#include <QSlider>
#include <QStackedWidget>
#include <QStyle>
#include <QToolButton>
#include <QVBoxLayout>

#include <algorithm>
#include <cmath>
#include <memory>
#include <vector>

namespace tukzmon {
namespace {

QString qs(const rust::String &s) { return QString::fromUtf8(s.data(), int(s.size())); }

// The style's own selection is a solid block meant for highlighted text, and in
// a dark theme that's white on bright blue. A tint of it under the plain text
// color reads in a light theme and a dark one.
void softenSelection(QWidget *w) {
	QPalette pal = w->palette();
	for (auto group : {QPalette::Active, QPalette::Inactive}) {
		const QColor base = pal.color(group, QPalette::Base);
		const QColor pick = pal.color(group, QPalette::Highlight);
		auto mix = [](float a, float b) { return a + (b - a) * 0.35f; };
		const QColor tint = QColor::fromRgbF(mix(base.redF(), pick.redF()), mix(base.greenF(), pick.greenF()),
			mix(base.blueF(), pick.blueF()));
		pal.setColor(group, QPalette::Highlight, tint);
		pal.setColor(group, QPalette::HighlightedText, pal.color(group, QPalette::Text));
	}
	w->setPalette(pal);
}

constexpr int SLIDER_STEPS = 1000;

// QScrollArea's own hint stops at about 24 lines, so a tall tab opened with a
// scroll bar it had no need for.
class FitScroll : public QScrollArea {
public:
	QSize sizeHint() const override {
		QSize s = QScrollArea::sizeHint();
		if (widget())
			s.setHeight(std::max(s.height(), widget()->sizeHint().height() + 2 * frameWidth()));
		return s;
	}
};

struct Row {
	RowInfo info;
	QLabel *label = nullptr;
	QCheckBox *check = nullptr;
	QSlider *slider = nullptr;
	QDoubleSpinBox *spin = nullptr;
	QComboBox *combo = nullptr;
	QLineEdit *line = nullptr;
	QPushButton *chip = nullptr;
	QToolButton *revert = nullptr;
	std::string id() const { return std::string(info.id); }
};

class Window : public QDialog {
public:
	explicit Window(Dialog &d) : d(d) {
		setWindowTitle(qs(dialog_title(d)));
		auto tabs = dialog_tabs(d);
		for (auto &r : dialog_rows(d)) {
			auto row = std::make_unique<Row>();
			row->info = r;
			rows.push_back(std::move(row));
		}

		tabList = new QListWidget;
		softenSelection(tabList);
		pages = new QStackedWidget;
		for (size_t i = 0; i < tabs.size(); i++) {
			bool parent = false;
			for (auto &t : tabs)
				parent |= t.parent == int(i);
			int depth = 0;
			for (int p = tabs[i].parent; p >= 0; p = tabs[size_t(p)].parent)
				depth++;
			auto *item = new QListWidgetItem(QString(depth * 4, ' ') + qs(tabs[i].label));
			tabList->addItem(item);
			tabPage.push_back(parent ? -1 : pages->addWidget(buildPage(int(i), tabs[i])));
		}
		const int tabWidth = tabList->sizeHintForColumn(0);
		tabList->setFixedWidth(tabWidth + 2 * tabList->frameWidth() + 24);
		for (int i = 0; i < tabList->count(); i++)
			tabList->item(i)->setSizeHint(QSize(tabWidth, tabList->fontMetrics().height() * 2));
		// one width for every number box, so the sliders all end in one column
		int numberWidth = 0;
		for (auto &r : rows)
			if (r->spin)
				numberWidth = std::max(numberWidth, r->spin->sizeHint().width());
		for (auto &r : rows)
			if (r->spin)
				r->spin->setFixedWidth(numberWidth);
		connect(tabList, &QListWidget::currentRowChanged, this, [this](int i) { showTab(i); });

		// Cancel stands a little apart from the 2 that save
		auto *cancel = new QPushButton("Cancel");
		applyButton = new QPushButton("&Apply");
		auto *closeButton = new QPushButton("Apply and &close");
		cancel->setToolTip("Discard every change and close Settings.");
		applyButton->setToolTip("Save changes now, without closing Settings.");
		closeButton->setToolTip("Save changes and close Settings.");
		closeButton->setDefault(true);
		connect(cancel, &QPushButton::clicked, this, &QDialog::reject);
		connect(applyButton, &QPushButton::clicked, this, [this] { save(); });
		connect(closeButton, &QPushButton::clicked, this, [this] {
			if (save())
				accept();
		});
		int gap = style()->layoutSpacing(QSizePolicy::PushButton, QSizePolicy::PushButton, Qt::Horizontal);
		if (gap < 0)
			gap = 6;
		auto *buttons = new QHBoxLayout;
		buttons->setSpacing(0);
		buttons->addStretch(1);
		buttons->addWidget(cancel);
		buttons->addSpacing(2 * gap);
		buttons->addWidget(applyButton);
		buttons->addSpacing(gap);
		buttons->addWidget(closeButton);

		auto *body = new QHBoxLayout;
		body->addWidget(tabList);
		body->addWidget(pages, 1);
		auto *outer = new QVBoxLayout(this);
		outer->addLayout(body, 1);
		outer->addLayout(buttons);

		tabList->setCurrentRow(0);
		refresh();
		qApp->installEventFilter(this);
	}

	// Enter is OK and Esc is Cancel, even from inside a field or on a checkbox.
	bool eventFilter(QObject *obj, QEvent *ev) override {
		if (ev->type() != QEvent::KeyPress)
			return false;
		auto *w = qobject_cast<QWidget *>(obj);
		if (!w || w->window() != this)
			return false;
		int key = static_cast<QKeyEvent *>(ev)->key();
		if (key == Qt::Key_Escape) {
			reject();
			return true;
		}
		if (key != Qt::Key_Return && key != Qt::Key_Enter)
			return false;
		// leaving a field is what commits a half-typed value
		if (QWidget *f = focusWidget())
			f->clearFocus();
		if (save())
			accept();
		return true;
	}

	int tabCount() const { return tabList->count(); }
	bool isLeaf(int i) const { return tabPage[size_t(i)] >= 0; }
	void select(int i) { tabList->setCurrentRow(i); }

private:
	Dialog &d;
	std::vector<std::unique_ptr<Row>> rows;
	std::vector<int> tabPage;
	QListWidget *tabList = nullptr;
	QStackedWidget *pages = nullptr;
	QPushButton *applyButton = nullptr;

	// a tab with tabs under it shows its first one
	void showTab(int i) {
		if (i < 0)
			return;
		for (int j = i; j < tabList->count(); j++) {
			if (tabPage[size_t(j)] >= 0) {
				pages->setCurrentIndex(tabPage[size_t(j)]);
				return;
			}
		}
	}

	QWidget *buildPage(int tab, const TabInfo &info) {
		auto *page = new QWidget;
		auto *grid = new QGridLayout(page);
		grid->setColumnStretch(1, 1);
		grid->setVerticalSpacing(6);
		int line = 0;
		for (auto &rp : rows) {
			Row &r = *rp;
			if (r.info.tab != tab)
				continue;
			QString label = qs(r.info.label);
			if (std::string(r.info.control) == "heading") {
				// a heading that only repeats the tab's name says nothing
				if (label == qs(info.label))
					continue;
				auto *h = new QLabel(label);
				QFont f = h->font();
				f.setBold(true);
				h->setFont(f);
				auto *rule = new QFrame;
				rule->setFrameShape(QFrame::HLine);
				rule->setFrameShadow(QFrame::Sunken);
				if (line > 0)
					grid->setRowMinimumHeight(line++, 10);
				grid->addWidget(h, line++, 0, 1, 3);
				grid->addWidget(rule, line++, 0, 1, 3);
				continue;
			}
			r.label = new QLabel(label);
			r.label->setContentsMargins(16 * r.info.indent, 0, 12, 0);
			grid->addWidget(r.label, line, 0);
			grid->addWidget(buildControl(r), line, 1);
			r.revert = new QToolButton;
			r.revert->setIcon(QIcon::fromTheme("edit-undo", style()->standardIcon(QStyle::SP_BrowserReload)));
			r.revert->setAutoRaise(true);
			connect(r.revert, &QToolButton::clicked, this, [this, &r] {
				reset_row(d, r.id());
				refresh();
			});
			grid->addWidget(r.revert, line, 2);
			line++;
		}
		grid->setRowStretch(line, 1);
		auto *scroll = new FitScroll;
		scroll->setWidget(page);
		scroll->setWidgetResizable(true);
		scroll->setFrameShape(QFrame::NoFrame);
		return scroll;
	}

	QDoubleSpinBox *numberBox(Row &r) {
		auto *spin = new QDoubleSpinBox;
		spin->setDecimals(r.info.whole ? 0 : 2);
		spin->setRange(std::isfinite(r.info.min) ? r.info.min : -1e9, std::isfinite(r.info.max) ? r.info.max : 1e9);
		spin->setKeyboardTracking(false);
		connect(spin, &QDoubleSpinBox::valueChanged, this, [this, &r](double v) {
			set_number(d, r.id(), v);
			refresh();
		});
		return spin;
	}

	QWidget *buildControl(Row &r) {
		const std::string c(r.info.control);
		if (c == "checkbox") {
			r.check = new QCheckBox;
			connect(r.check, &QCheckBox::toggled, this, [this, &r](bool on) {
				set_bool(d, r.id(), on);
				refresh();
			});
			return r.check;
		}
		if (c == "slider") {
			r.slider = new QSlider(Qt::Horizontal);
			r.slider->setRange(0, SLIDER_STEPS);
			r.slider->setMinimumWidth(180);
			connect(r.slider, &QSlider::valueChanged, this, [this, &r](int v) {
				set_slider(d, r.id(), double(v) / SLIDER_STEPS);
				refresh();
			});
			r.spin = numberBox(r);
			auto *w = new QWidget;
			auto *h = new QHBoxLayout(w);
			h->setContentsMargins(0, 0, 0, 0);
			h->addWidget(r.slider, 1);
			h->addWidget(r.spin);
			return w;
		}
		if (c == "number") {
			r.spin = numberBox(r);
			return r.spin;
		}
		if (c == "dropdown") {
			r.combo = new QComboBox;
			for (auto &l : r.info.option_labels)
				r.combo->addItem(qs(l));
			connect(r.combo, &QComboBox::activated, this, [this, &r](int i) {
				if (i >= 0 && size_t(i) < r.info.options.size())
					set_text(d, r.id(), std::string(r.info.options[size_t(i)]));
				refresh();
			});
			return r.combo;
		}
		r.line = new QLineEdit;
		connect(r.line, &QLineEdit::editingFinished, this, [this, &r] {
			set_text(d, r.id(), r.line->text().toStdString());
			refresh();
		});
		if (c != "color")
			return r.line;

		r.line->setValidator(new QRegularExpressionValidator(QRegularExpression("#?[0-9a-fA-F]{0,6}"), r.line));
		r.line->setMaximumWidth(r.line->fontMetrics().horizontalAdvance("#DDDDDDD") + 16);
		r.chip = new QPushButton;
		r.chip->setFixedSize(36, r.line->sizeHint().height());
		r.chip->setToolTip("Pick a color.");
		connect(r.chip, &QPushButton::clicked, this, [this, &r] { pickColor(r); });
		auto *w = new QWidget;
		auto *h = new QHBoxLayout(w);
		h->setContentsMargins(0, 0, 0, 0);
		h->addWidget(r.chip);
		h->addWidget(r.line);
		h->addStretch(1);
		return w;
	}

	// Edits show on the row at once, and Cancel puts the old color back.
	void pickColor(Row &r) {
		QString was = qs(row_state(d, r.id()).text);
		QColorDialog pick(QColor(was), this);
		pick.setWindowTitle(r.label->text());
		connect(&pick, &QColorDialog::currentColorChanged, this, [this, &r](const QColor &c) {
			set_text(d, r.id(), c.name().toStdString());
			refresh();
		});
		if (pick.exec() == QDialog::Accepted)
			set_text(d, r.id(), pick.selectedColor().name().toStdString());
		else
			set_text(d, r.id(), was.toStdString());
		refresh();
	}

	bool save() {
		if (!is_dirty(d))
			return true;
		auto err = apply(d);
		refresh();
		if (err.empty())
			return true;
		QMessageBox::warning(this, "Settings weren't saved", qs(err));
		return false;
	}

	void refresh() {
		for (auto &rp : rows) {
			Row &r = *rp;
			if (!r.label)
				continue;
			auto st = row_state(d, r.id());
			QString tip = qs(st.tip);
			QString text = qs(st.text);
			r.label->setToolTip(tip);
			r.revert->setEnabled(st.can_reset);
			r.revert->setToolTip(st.can_reset ? "Back to the default." : "");
			if (r.check) {
				QSignalBlocker b(r.check);
				r.check->setChecked(st.on);
				r.check->setToolTip(tip);
			}
			if (r.slider) {
				QSignalBlocker b(r.slider);
				r.slider->setValue(int(std::lround(st.slider * SLIDER_STEPS)));
				r.slider->setToolTip(tip);
			}
			if (r.spin) {
				QSignalBlocker b(r.spin);
				r.spin->setValue(st.number);
				r.spin->setToolTip(tip);
			}
			if (r.combo) {
				QSignalBlocker b(r.combo);
				for (size_t i = 0; i < r.info.options.size(); i++)
					if (qs(r.info.options[i]) == text)
						r.combo->setCurrentIndex(int(i));
				r.combo->setToolTip(tip);
			}
			if (r.line) {
				QSignalBlocker b(r.line);
				r.line->setText(text);
				r.line->setToolTip(tip);
			}
			if (r.chip)
				r.chip->setStyleSheet(QString("background-color: %1; border: 1px solid palette(mid);").arg(text));
		}
		applyButton->setEnabled(is_dirty(d));
	}
};

} // namespace

std::int32_t run_dialog(Dialog &d, rust::Str snapshot) {
	static int argc = 1;
	static char name[] = "tukzmon-settings";
	static char *argv[] = {name, nullptr};
	QApplication app(argc, argv);
	QApplication::setApplicationName("tukzmon-settings");
	Window w(d);
	QSize size = w.sizeHint().expandedTo(QSize(560, 420));
	if (const QScreen *screen = w.screen())
		size = size.boundedTo(screen->availableGeometry().size());
	w.resize(size);
	if (snapshot.empty())
		return w.exec() == QDialog::Accepted ? 0 : 1;

	// one picture per tab with rows, as base-1.png, base-2.png, and so on
	QString base = QString::fromUtf8(snapshot.data(), int(snapshot.size()));
	if (base.endsWith(".png"))
		base.chop(4);
	w.show();
	int n = 0;
	for (int i = 0; i < w.tabCount(); i++) {
		if (!w.isLeaf(i))
			continue;
		w.select(i);
		app.processEvents();
		if (!w.grab().save(QString("%1-%2.png").arg(base).arg(++n)))
			return 3;
	}
	return 0;
}

} // namespace tukzmon
